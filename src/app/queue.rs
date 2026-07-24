//! The playback queue: membership, ordering, and what plays next.
//!
//! Shuffle is a *cycle*, not a per-step coin flip: every track plays once
//! before any repeats (`shuffle_played`), so exhausting the cycle with
//! repeat off ends the queue instead of drawing forever.

use super::*;

impl App {
    /// Adiciona a faixa selecionada ao fim da fila (sem interromper a atual).
    pub fn enqueue_selected(&mut self) {
        let track = match self.section {
            // Nos resultados mistos, apenas músicas podem ir para a fila.
            Section::Buscar if self.search_mixed => {
                match self
                    .list_state
                    .selected()
                    .and_then(|i| self.search_hit_at(i))
                {
                    Some(SearchHit::Song(i)) => self.songs.get(i).cloned(),
                    Some(_) => {
                        self.status = "Somente músicas podem ser adicionadas à fila.".to_string();
                        return;
                    }
                    None => None,
                }
            }
            Section::Buscar => self
                .list_state
                .selected()
                .and_then(|i| self.songs.get(i))
                .cloned(),
            Section::Inicio => self
                .list_state
                .selected()
                .and_then(|i| self.home_view().flat_card(i).cloned())
                .and_then(|card| match card.payload {
                    HomeCardPayload::Track(track) => Some(track),
                    HomeCardPayload::Collection(_) => None,
                }),
            Section::Fila => None, // já está na fila
            _ => None,
        };
        let Some(track) = track else { return };
        let title = track.title.clone();
        self.queue.push(track);
        // Nada tocando ainda? começa a tocar o que foi enfileirado.
        if self.current.is_none() {
            self.queue_index = Some(self.queue.len() - 1);
            self.start_current();
        } else {
            // Recalcula o próximo (a fila mudou de tamanho).
            self.recompute_next();
            self.status = format!(
                "➕ \"{title}\" adicionada à fila ({} na fila).",
                self.queue.len()
            );
        }
    }

    /// Remove a faixa selecionada da fila. A faixa em reprodução não pode
    /// ser removida (pule com `n` ou pare com `s`): mantê-la evita um estado
    /// ambíguo de "tocando algo que não está na fila".
    pub fn queue_remove_selected(&mut self) {
        let Some(idx) = self.list_state.selected().filter(|&i| i < self.queue.len()) else {
            return;
        };
        if self.queue_index == Some(idx) && self.current.is_some() {
            self.status = "A faixa em reprodução não sai da fila — pule com n.".to_string();
            return;
        }
        let removed = self.queue.remove(idx);
        if let Some(qi) = self.queue_index {
            if idx < qi {
                self.queue_index = Some(qi - 1);
            } else if idx == qi {
                // Só alcançável com a reprodução parada (guarda acima).
                self.queue_index = None;
            }
        }
        let len = self.queue.len();
        self.list_state.select((len > 0).then(|| idx.min(len - 1)));
        self.recompute_next();
        self.status = format!("Removida da fila: {}", removed.title);
    }

    /// Move a faixa selecionada uma posição para cima/baixo na fila,
    /// levando a seleção junto e repontando o índice da faixa atual se ela
    /// participar da troca.
    pub fn queue_move_selected(&mut self, delta: isize) {
        let Some(idx) = self.list_state.selected().filter(|&i| i < self.queue.len()) else {
            return;
        };
        let target = idx as isize + delta;
        if target < 0 || target as usize >= self.queue.len() {
            return;
        }
        let target = target as usize;
        self.queue.swap(idx, target);
        if let Some(qi) = self.queue_index {
            if qi == idx {
                self.queue_index = Some(target);
            } else if qi == target {
                self.queue_index = Some(idx);
            }
        }
        self.list_state.select(Some(target));
        self.recompute_next();
    }

    /// Limpa a fila, preservando apenas a faixa em reprodução (se houver).
    pub fn queue_clear(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        match self.current.clone() {
            Some(current) => {
                self.queue = vec![current];
                self.queue_index = Some(0);
                self.list_state.select(Some(0));
            }
            None => {
                self.queue.clear();
                self.queue_index = None;
                self.list_state.select(None);
            }
        }
        self.next_index = None;
        self.shuffle_played.clear();
        self.status = "Fila limpa.".to_string();
    }

    /// Gera o próximo número pseudoaleatório (xorshift64).
    pub(super) fn next_rand(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        x
    }

    /// Alterna a reprodução aleatória.
    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.shuffle_played.clear();
        // A faixa atual conta como já tocada no ciclo que começa agora.
        if self.shuffle {
            if let Some(t) = &self.current {
                self.shuffle_played.insert(t.video_id.clone());
            }
        }
        self.status = if self.shuffle {
            "🔀 Aleatório ativado.".to_string()
        } else {
            "➡ Aleatório desativado.".to_string()
        };
        // Recalcula o próximo com base no novo modo.
        self.recompute_next();
    }

    /// Alterna o modo de repetição (Off → Todos → Um).
    pub fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.next();
        self.status = format!("🔁 Repetição: {}.", self.repeat.label());
        self.recompute_next();
    }

    /// Recalcula `next_index` a partir da posição atual, respeitando os
    /// modos de shuffle/repeat vigentes.
    pub(super) fn recompute_next(&mut self) {
        self.next_index = self
            .queue_index
            .and_then(|idx| self.compute_next(idx, self.repeat != RepeatMode::Off));
    }

    /// Calcula o índice da próxima faixa a partir de `idx`.
    ///
    /// `allow_wrap` indica se, ao chegar ao fim em ordem sequencial, deve voltar
    /// ao início. No modo aleatório, sorteia entre as faixas ainda não tocadas
    /// no ciclo atual (ver `shuffle_played`); esgotado o ciclo, `allow_wrap`
    /// decide entre começar outro ciclo ou encerrar a fila.
    pub(super) fn compute_next(&mut self, idx: usize, allow_wrap: bool) -> Option<usize> {
        let len = self.queue.len();
        if len == 0 {
            return None;
        }
        if len == 1 {
            return if allow_wrap { Some(0) } else { None };
        }
        if self.shuffle {
            let unplayed: Vec<usize> = (0..len)
                .filter(|&i| i != idx && !self.shuffle_played.contains(&self.queue[i].video_id))
                .collect();
            if !unplayed.is_empty() {
                let pick = (self.next_rand() % unplayed.len() as u64) as usize;
                return Some(unplayed[pick]);
            }
            if !allow_wrap {
                return None;
            }
            // Novo ciclo: tudo volta a valer, menos repetir a atual em
            // seguida.
            self.shuffle_played.clear();
            let mut n = idx;
            while n == idx {
                n = (self.next_rand() % len as u64) as usize;
            }
            Some(n)
        } else if idx + 1 < len {
            Some(idx + 1)
        } else if allow_wrap {
            Some(0)
        } else {
            None
        }
    }

    /// Pré-baixa (em background) o áudio da faixa de índice `idx` para o cache.
    pub(super) fn prefetch(&self, idx: usize) {
        let Some(track) = self.queue.get(idx) else {
            return;
        };
        if track.video_id.is_empty() {
            return;
        }
        let track = track.clone();
        let provider = Arc::clone(&self.provider);
        tokio::task::spawn_blocking(move || {
            let _ = provider.resolve_playable(&track);
        });
    }
}

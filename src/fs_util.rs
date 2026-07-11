//! Utilitários de arquivo compartilhados entre `config` e `app`.

use std::io;
use std::path::Path;

/// Escreve `contents` em `path` de forma atômica: grava em um arquivo
/// temporário no mesmo diretório e o renomeia sobre o destino. Evita deixar
/// um arquivo pela metade se o processo for interrompido no meio da escrita.
pub fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = tempfile::NamedTempFile::new_in(dir)?;
    std::fs::write(tmp.path(), contents)?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

use std::path::PathBuf;

/// Everything that can go wrong during an installation.
///
/// Each variant carries enough context to be shown to the user as-is, in the
/// installer's error panel.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("commande échouée : {cmd}\n{stderr}")]
    Command { cmd: String, stderr: String },

    #[error("erreur d'E/S sur {path} : {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("configuration invalide : {0}")]
    Config(String),

    #[error("disque inutilisable : {0}")]
    Layout(String),

    #[error("limine : {0}")]
    Limine(String),

    #[error("btrfs : {0}")]
    Btrfs(String),

    #[error("aucune étape d'installation n'est en cours")]
    NotRunning,
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

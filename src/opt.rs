use std::{
    env::current_dir,
    io,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

use structopt::{
    clap::AppSettings::{ColoredHelp, DeriveDisplayOrder},
    StructOpt,
};

#[derive(StructOpt, Debug)]
#[structopt(name = "intray", about = "An intray to facilitate collecting files.")]
#[structopt(global_settings(&[ColoredHelp, DeriveDisplayOrder]))]
pub struct Opt {
    /// IP address to bind on
    #[structopt(short = "a", long = "ip-addr", default_value = "::")]
    ip_addr: IpAddr,

    /// Directory to store received files
    #[structopt(short = "d", long = "dir", parse(from_os_str), default_value = "./")]
    dir: PathBuf,

    /// Credentials for HTTP Basic Auth in the format "USERNAME:PASSWD"
    #[structopt(short = "c", long = "credentials", env = "CREDENTIALS")]
    auth_credentials: Vec<String>, // TODO: HashSet?

    /// Realm to send in `WWW-Authenticate` HTTP header for HTTP Basic Auth
    #[structopt(short = "r", long = "realm", default_value = "Intray")]
    pub auth_realm: String,

    /// Port to bind on
    #[structopt(name = "PORT", default_value = "8080")]
    port: u16,
}

impl Opt {
    pub fn dir(&self) -> &Path {
        self.dir.as_ref()
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip_addr, self.port)
    }

    pub fn is_auth_enabled(&self) -> bool {
        return !self.auth_credentials.is_empty()
    }

    pub fn credentials_match(&self, credentials: impl AsRef<str>) -> bool {
        let credentials = credentials.as_ref();
        self.auth_credentials.iter().any(|c| c == credentials)
    }

    pub fn warn_if_invalid(&self) {
        // TODO: Integrate this fn to structopt validator (?)
        if !self.dir.exists() {
            warn!(
                "{:?} does not exist.",
                canonicalize_path(&self.dir).unwrap_or_else(|_| self.dir.clone())
            );
        } else if !self.dir.is_dir() {
            warn!(
                "{:?} is not a directory.",
                canonicalize_path(&self.dir).unwrap_or_else(|_| self.dir.clone())
            );
            //, self.dir.canonicalize().unwrap_or_else(|_| self.dir.clone()));
        }
        // Path::canonicalize is not proper here because it check the existence of the file of the path.

        if self.auth_credentials.len() >= 10 {
            warn!("Too many authentication credentials specified. Intray may suffer from performance penalty.")
        }
    }
}

lazy_static! {
    pub static ref OPT: Opt = Opt::from_args();
}

fn canonicalize_path(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    // TODO: it is only joining without "all intermediate components normalized and symbolic links resolved".
    Ok(current_dir()?.join(path.as_ref()))
}

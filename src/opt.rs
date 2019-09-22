use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::env::current_dir;
use std::io;

use structopt::clap::AppSettings::ColoredHelp;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "intray", about = "An simple intray to help receiving files.")]
#[structopt(global_settings(&[ColoredHelp]))]
pub struct Opt {
    /// IP address to bind on
    #[structopt(short = "a", long = "ip-addr", default_value = "::")]
    ip_addr: IpAddr,

    /// Directory to store received files
    #[structopt(short = "d", long = "dir", parse(from_os_str), default_value = "./")]
    dir: PathBuf,

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

    pub fn warn_if_invalid(&self) {
        // TODO: Integrate this fn to structopt validator (?)
        if !self.dir.exists() {
            warn!("{:?} does not exist.", canonicalize_path(&self.dir).unwrap_or_else(|_| self.dir.clone()));
        } 
        else if !self.dir.is_dir() {
            warn!("{:?} is not a directory.", canonicalize_path(&self.dir).unwrap_or_else(|_| self.dir.clone()));
            //, self.dir.canonicalize().unwrap_or_else(|_| self.dir.clone()));
        }
        // Path::canonicalize is not proper here because it check the existence of the file of the path. 
    }
}

lazy_static! {
    pub static ref OPT: Opt = Opt::from_args();
}

fn canonicalize_path(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    // TODO: it is only joining without "all intermediate components normalized and symbolic links resolved". 
    Ok(current_dir()?.join(path.as_ref()))
}
use std::net::{IpAddr, SocketAddr};
use std::path::{PathBuf, Path};

use structopt::StructOpt;
use structopt::clap::AppSettings::ColoredHelp;


#[derive(StructOpt, Debug)]
#[structopt(
    name = "intray",
    about = "An simple intray to help receiving files.",
)]
#[structopt(raw(setting = "ColoredHelp"))]
pub struct Opt {
    /// IP address to bind on
    #[structopt(short = "a", long="ip-addr", default_value = "0.0.0.0")]
    ip_addr: IpAddr,
    
    /// Directory to store received files
    #[structopt(short = "d", long="dir", parse(from_os_str), default_value = "./")]
    dir: PathBuf,
    
    /// Port to bind on
    #[structopt(name = "PORT", default_value = "8080")]
    port: u16
}

impl Opt {
    pub fn dir(&self) -> &Path {
        self.dir.as_ref()
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip_addr, self.port)
    }
}
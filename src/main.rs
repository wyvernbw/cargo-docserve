#![feature(trim_prefix_suffix)]
use std::{
    fs,
    io::{BufRead, BufReader},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
    sync::atomic::AtomicBool,
};

use clap::Parser;
use tiny_http::{Response, Server};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error + 'static>>;

#[derive(clap::Parser)]
struct CliArgs {
    #[arg(short, default_value_t = 8080)]
    port: u16,
    #[arg(short)]
    address: Option<IpAddr>,
}

fn main() -> Result<ExitCode> {
    let cli = CliArgs::parse();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut docs = Command::new("cargo");
    docs.args(["doc"]);
    docs.args(&args);
    let mut child = docs.spawn()?;
    let exit_code = child.wait()?.code();

    let mut docs = Command::new("cargo");
    docs.args(["doc"]);
    docs.args(&args);
    docs.stderr(Stdio::piped());
    let mut child = docs.spawn()?;
    let stderr = child.stderr.take().expect("must pipe stderr");
    let stderr = BufReader::new(stderr);
    let index_path = get_docs_path(stderr)?;
    let project_root = index_path.parent().ok_or("expected dirname")?;
    let docs_root = project_root.parent().ok_or("expected dirname")?;

    let server = Server::http(SocketAddr::new(
        cli.address
            .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))),
        cli.port,
    ))
    .unwrap();
    static QUIT: AtomicBool = AtomicBool::new(false);
    let addr = server.server_addr();
    println!("\x1b[1;92mServing docs\x1b[0m at \x1b]8;;http://{addr}\x1b\\{addr}\x1b]8;;\x1b\\");
    println!("\x1b[37mpress ^C to quit.\x1b[0m");

    std::thread::scope(|scope| -> Result<()> {
        let (quit_tx, quit_rx) = std::sync::mpsc::channel();
        ctrlc::set_handler(move || {
            quit_tx.send(()).unwrap();
        })?;

        let server = &server;
        scope.spawn(move || {
            quit_rx.recv().unwrap();
            QUIT.store(false, std::sync::atomic::Ordering::SeqCst);
            server.unblock();
        });

        let index_path = &index_path;
        let docs_root = &docs_root;
        for request in server.incoming_requests() {
            scope.spawn(move || {
                let handler = || -> Result<()> {
                    let path = match request.url() {
                        "/" => index_path.as_path(),
                        s => {
                            let s = s.trim().trim_prefix("/");
                            let root = docs_root;
                            &root.join(s)
                        }
                    };

                    let file = fs::File::open(path);
                    match file {
                        Ok(file) => {
                            let res = Response::from_file(file);

                            request.respond(res)?;
                        }
                        Err(_) => request.respond(Response::empty(404))?,
                    }

                    Ok(())
                };
                handler().unwrap();
            });
        }
        Ok(())
    })?;

    Ok((exit_code.unwrap_or(0) as u8).into())
}

fn get_docs_path(reader: impl BufRead) -> Result<PathBuf> {
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.starts_with("Generated") {
            let Some((_, path)) = line.split_once(" ") else {
                continue;
            };
            return Ok(path.to_string().into());
        }
    }
    Err("doc path not found".to_string().into())
}

#[allow(dead_code)]
enum TestEnum {}

use ansi_term::Colour::{Fixed, Green, Yellow};
use clap::Parser;
use log::{error, info, LevelFilter};
use northstar_rcon_client::{AuthError, ClientRead, ClientWrite, NotAuthenticatedClient};
use rpassword::read_password;
use simplelog::{ColorChoice, Config, TermLogger, TerminalMode};
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Address of the Northstar server, e.g. `127.0.0.1:37015`.
    address: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ! {
    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .unwrap();

    let args = Args::parse();

    // Try to parse address with port, if that fails try to parse without and default to 37015.
    let socket_addr: SocketAddr = match args.address.parse() {
        Ok(addr) => addr,
        Err(_) => {
            let ip_addr: IpAddr = match args.address.parse() {
                Ok(addr) => addr,
                Err(_) => {
                    eprintln!("Invalid address: {}", args.address);
                    std::process::exit(1);
                }
            };
            SocketAddr::new(ip_addr, 37015)
        }
    };

    let mut client = match NotAuthenticatedClient::new(socket_addr).await {
        Ok(client) => client,
        Err(err) => {
            error!("Connection failed: {}", err);
            std::process::exit(1);
        }
    };

    let (client_read, client_write) = loop {
        print!("{}'s password: ", socket_addr);
        std::io::stdout().flush().unwrap();
        let password = read_password().unwrap();

        match client.authenticate(&password).await {
            Ok(halves) => break halves,
            Err((new_client, AuthError::InvalidPassword)) => {
                println!("Invalid password.");
                client = new_client;
            }
            Err((_, AuthError::Banned)) => {
                eprintln!("You are banned from this server.");
                std::process::exit(1);
            }
            Err((_, AuthError::Fatal(err))) => {
                error!("Connection failed: {}", err);
                std::process::exit(1);
            }
        };
    };

    info!(
        "Connected. View builtins with `!help`. {} {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let prompt = Prompt { socket_addr };

    // Start logging incoming lines
    tokio::spawn(log_loop(client_read, prompt.clone()));

    // Start receiving REPL inputs
    repl_loop(client_write, prompt).await
}

#[derive(Clone)]
struct Prompt {
    socket_addr: SocketAddr,
}

impl Display for Prompt {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}> ", Fixed(10).paint(self.socket_addr.to_string()))
    }
}

async fn log_loop(mut client_read: ClientRead, prompt: Prompt) -> ! {
    loop {
        match client_read.receive_console_log().await {
            Ok(log) => {
                print!("\r");
                info!("{}", log);
                print!("{}", prompt);
                std::io::stdout().flush().unwrap();
            }
            Err(err) => {
                eprint!("\r");
                error!("Connection closed: {}", err);
                std::process::exit(1);
            }
        }
    }
}

async fn repl_loop(mut client_write: ClientWrite, prompt: Prompt) -> ! {
    let mut input_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        print!("{}", prompt);
        std::io::stdout().flush().unwrap();

        let line = match input_lines.next_line().await.unwrap() {
            Some(line) => line,
            None => continue,
        };
        let line = line.trim();

        let result = if let Some(builtin) = line.strip_prefix('!') {
            if builtin == "help" {
                println!(
                    "{} {}",
                    Green.paint(env!("CARGO_PKG_NAME")),
                    env!("CARGO_PKG_VERSION")
                );
                println!();
                println!("{}", Yellow.paint("BUILTINS"));
                println!("    !help                View this help listing");
                println!("    !enable console      Enable server console logging");
                println!(
                    "    !set {}     Set a ConVar on the server",
                    Green.paint("<VAR> <VAL>")
                );
                println!(
                    "    {}  Run a command on the server",
                    Green.paint("<COMMAND> [ARGS...]")
                );

                Ok(())
            } else if builtin == "enable console" {
                client_write.enable_console_logs().await
            } else if let Some(set_query) = builtin.strip_prefix("set ") {
                client_write.set_value(set_query.trim()).await
            } else {
                eprintln!("Unknown builtin.");
                Ok(())
            }
        } else {
            client_write.exec_command(line).await
        };

        if let Err(err) = result {
            eprintln!("An error occurred: {}", err);
        }
    }
}

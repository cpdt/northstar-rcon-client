use crate::shell::{new_shell, ShellRead, ShellWrite};
use clap::Parser;
use crossterm::style::{Color, Stylize};
use rpassword::prompt_password;
use std::fmt::{Display, Formatter};
use std::net::{SocketAddr, ToSocketAddrs};
use northstar_rcon_client::{AuthError, Event, Request};
use northstar_rcon_client::sync::{ClientRead, ClientWrite, connect};

mod shell;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Address of the Northstar server, e.g. `127.0.0.1:37015`.
    address: String,

    /// Name to display for the server in the prompt.
    #[clap(short, long)]
    name: Option<String>,

    /// Authenticate automatically with a password in a file.
    #[clap(short, long)]
    pass_file: Option<String>,

    /// Force non-interactive script mode, even in interactive terminals.
    #[clap(long)]
    script_mode: bool,
}

fn main() -> ! {
    let args = Args::parse();

    // Try to parse address with port, if that fails try to parse without and default to 37015.
    let socket_addr: SocketAddr = match parse_string_addr(&args.address) {
        Ok(addr) => addr,
        Err(err) => {
            eprintln!("Invalid address {}: {}", args.address, err);
            proc_exit::Code::SERVICE_UNAVAILABLE.process_exit();
        }
    };

    // Read the automated password, if one was supplied somehow.
    let automated_password =
        args.pass_file
            .as_ref()
            .map(|pass_file| match std::fs::read_to_string(pass_file) {
                Ok(pass) => pass.trim().to_string(),
                Err(err) => {
                    eprintln!("Can't read pass file: {}", err);
                    proc_exit::Code::IO_ERR.process_exit();
                }
            });

    let name = args.name.unwrap_or_else(|| socket_addr.to_string());

    let mut client = match connect(socket_addr) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Connection failed: {}", err);
            proc_exit::Code::SERVICE_UNAVAILABLE.process_exit();
        }
    };

    let (client_read, client_write) = match &automated_password {
        Some(pass) => match client.authenticate(pass) {
            Ok(halves) => halves,
            Err((_, err)) => {
                eprintln!("Authentication failed: {}", CliAuthError(err));
                proc_exit::Code::SERVICE_UNAVAILABLE.process_exit();
            }
        },
        None => loop {
            let pass = prompt_password(format!("{}'s password: ", name)).unwrap();

            match client.authenticate(&pass) {
                Ok(halves) => break halves,
                Err((new_client, err)) => {
                    let err = CliAuthError(err);
                    eprintln!("{}", err);

                    if err.is_fatal() {
                        proc_exit::Code::SERVICE_UNAVAILABLE.process_exit();
                    } else {
                        client = new_client;
                    }
                }
            }
        },
    };

    let (shell_read, shell_write) = new_shell(format!("{}> ", name), args.script_mode);

    // Start handling events
    let event_write = shell_write.clone();
    std::thread::spawn(move || event_thread(client_read, event_write));

    // Start receiving REPL inputs
    repl_thread(client_write, shell_read, shell_write);
}

fn parse_socket_addr(to: impl ToSocketAddrs) -> std::io::Result<SocketAddr> {
    to.to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
}

fn parse_string_addr(addr: &str) -> std::io::Result<SocketAddr> {
    // Try parsing with port.
    if let Ok(socket_addr) = parse_socket_addr(addr) {
        return Ok(socket_addr);
    }

    // Try parsing with a default port of 37015.
    parse_socket_addr((addr, 37015))
}

struct CliAuthError(AuthError);

impl CliAuthError {
    fn is_fatal(&self) -> bool {
        match &self.0 {
            AuthError::InvalidPassword => false,
            AuthError::Banned | AuthError::Fatal(_) => true,
        }
    }
}

impl Display for CliAuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            AuthError::InvalidPassword => write!(f, "Invalid password."),
            AuthError::Banned => write!(f, "You are banned from this server."),
            AuthError::Fatal(err) => write!(f, "Connection failed: {}", err),
        }
    }
}

fn event_thread(mut client_read: ClientRead, mut stdout: ShellWrite) -> ! {
    loop {
        match client_read.receive() {
            Ok(Event::ConsoleLog { msg }) => writeln!(stdout.out(), "{}", msg).unwrap(),
            Err(err) => {
                eprintln!("Connection closed: {}", err);
                proc_exit::Code::SERVICE_UNAVAILABLE.process_exit();
            }
        }
    }
}

fn repl_thread(mut client_write: ClientWrite, mut stdin: ShellRead, mut stdout: ShellWrite) -> ! {
    loop {
        let line = stdin.read_line();
        let line = line.trim();

        let request = match line.strip_prefix('!') {
            // No builtin prefix, execute as a command
            None => Some(Request::ExecCommand { cmd: line }),

            // Builtin prefix, match against builtins
            Some(builtin) => {
                if builtin == "help" {
                    writeln!(
                        stdout.err(),
                        r#"{} {}
{}
    {}                   View this help listing
    {}         Enable server console logging
    {}                   Quit this session
    {}        Set a ConVar on the server
    {}     Run a command on the server"#,
                        env!("CARGO_PKG_NAME").with(Color::DarkGreen),
                        env!("CARGO_PKG_VERSION"),
                        "BUILTINS:".with(Color::DarkYellow),
                        "!help".with(Color::DarkGreen),
                        "!quit".with(Color::DarkGreen),
                        "!enable console".with(Color::DarkGreen),
                        "!set <VAR> <VAL>".with(Color::DarkGreen),
                        "<COMMAND> [ARGS...]".with(Color::DarkGreen)
                    )
                        .unwrap();

                    None
                } else if builtin == "quit" {
                    eprintln!();
                    proc_exit::Code::SUCCESS.process_exit();
                } else if builtin == "enable console" {
                    Some(Request::EnableConsoleLogs)
                } else if let Some(set_query) = builtin.strip_prefix("set ") {
                    match set_query.find(' ') {
                        Some(separator_index) => {
                            let var = set_query[..separator_index].trim();
                            let val = set_query[separator_index + 1..].trim();
                            Some(Request::SetValue { var, val })
                        }
                        None => {
                            writeln!(stdout.err(), "Usage: !set <VAR> <VAL>").unwrap();
                            None
                        }
                    }
                } else {
                    writeln!(stdout.err(), "Unknown builtin.").unwrap();
                    None
                }
            }
        };

        if let Some(request) = request {
            let res = client_write.send(request);
            if let Err(err) = res {
                writeln!(stdout.err(), "An error occurred: {}", err).unwrap();
            }
        }
    }
}

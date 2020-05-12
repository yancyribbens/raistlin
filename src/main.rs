#![warn(rust_2018_idioms)]
#![feature(str_strip)]

use spellcheck::Speller;
use std::fs;

use tokio::net::{TcpStream};
use tokio::stream::{StreamExt};
use tokio::prelude::*;

use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

static NICK: &str = "raistlin";
static CHANNEL: &str = "#didnt";
static ADDR: &str = "irc.w3.org:6667";

//"PING :public-irc.w3.org\r\n"
#[derive(PartialEq, Debug)]
pub struct Ping {
    server: String
}

#[derive(PartialEq, Debug)]
enum Command {
    PING(Ping)
}

fn parse_command(event_str: String) -> Option<Command> {
    let cmd_vec: Vec<&str> = event_str.split(" :").collect();

    let mut cmd: Option<Command> = None;
    if cmd_vec.len() == 2 {
        let cmd_header: Vec<&str> = cmd_vec[0].split(" ").collect();
        let cmd_body: Option<&str> = cmd_vec[1].strip_suffix("\r\n");

        if cmd_body.is_some() {
            let body: String = cmd_body.unwrap().to_string();

            cmd = match &cmd_header[..] {
                ["PING"] => Some( Command::PING ( Ping { server: body } ) ),
                _ => None
            }
        }
    }

    cmd
}

pub struct Nick {
    nick: String
}

pub struct User {
    user: String,
    mode: String,
    unused: String,
    realname: String
}

pub struct Registration {
    user: User,
    nick: Nick
}

impl Registration {
    fn create_registration_str(&self) -> String {
        let user = &self.user;
        let nick = &self.nick;

        format!(
            "USER {user} {mode} {unused} {realname}\nNICK {nick}\n",
            user=user.user,
            mode=user.mode,
            unused=user.unused,
            realname=user.realname,
            nick=nick.nick
        )
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let mut speller = Speller {
        letters: "abcdefghijklmnopqrstuvwxyz".to_string(),
        n_words: HashMap::new()
    };

    let contents = fs::read_to_string("./src/training.txt")
        .expect("Something went wrong reading the file");
    speller.train(&contents);

    let mut stream = TcpStream::connect(ADDR).await?;

    let u = User {
        user: String::from("guest"),
        mode: String::from("tolmoon"),
        unused: String::from("tolsun"),
        realname: String::from(":Ronnie Regan")
    };

    let n = Nick {
        nick: String::from(NICK)
    };

    let r = Registration {
        user: u,
        nick: n
    };

    stream.write_all(&r.create_registration_str().as_bytes()).await;
    stream.write_all(b"JOIN #didnt\n").await;

    loop {
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await?;
        let recieved = String::from_utf8(buf[0..n].to_vec())?;

        let cmd = parse_command(recieved);

        if cmd.is_some() {
            let c = cmd.unwrap();

            match c {
                Command::PING(p) => {
                    let Ping { server: s } = p;
                    println!("got pinged from server: {}", s);
                    let response = format!("PONG {}\n", s);
                    stream.write_all(response.as_bytes());
                },
                _ => ()
            }
        };
    };

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_parse_pong() {
        let expected_cmd = 
            Command::PING( Ping { server: "public-irc.w3.org".to_string() } );
        let cmd_str = "PING :public-irc.w3.org\r\n".to_string();
        let cmd = parse_command(cmd_str).unwrap();
        assert_eq!(cmd, expected_cmd);
    }
}

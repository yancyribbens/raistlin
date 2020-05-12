#![warn(rust_2018_idioms)]
#![feature(str_strip)]

use spellcheck::Speller;
use async_trait::async_trait;
use git2::Repository;
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
static SCRIBE: &str = ":yancy!~root";

//"anubis@public.cloak PRIVMSG #didnt :hello there\r\n"
#[derive(PartialEq, Debug)]
pub struct Message {
    sender: String,
    channel: String,
    text: String
}

//"PING :public-irc.w3.org\r\n"
#[derive(PartialEq, Debug)]
pub struct Ping {
    server: String
}

#[derive(PartialEq, Debug)]
pub enum Command {
    PRIVMSG(Message),
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
                [sender, "PRIVMSG", channel] => Some(
                    Command::PRIVMSG ( Message {
                        sender: sender.to_string(),
                        channel: channel.to_string(),
                        text: body
                    })) ,
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

pub struct SpellCheck {
    speller: Speller 
}

impl SpellCheck {
    pub fn new(corpus: String) -> SpellCheck {
        let mut speller = Speller {
            letters: "abcdefghijklmnopqrstuvwxyz".to_string(),
            n_words: HashMap::new()
        };
        
        speller.train(&corpus);

        SpellCheck {
            speller: speller
        }
    }

    pub fn correct(&mut self, text: &String) -> Vec<String> {
        let text_vec: Vec<&str> = text.split(" ").collect();
        let normalized_vec: Vec<String> = text_vec
            .into_iter()
            .filter( |t| t.len() > 2)
            .map( |m| m.to_string())
            .collect();

        let corrected: Vec<String> = normalized_vec 
            .iter()
            .map( |t| self.speller.correct(&t) )
            .collect();
        
        normalized_vec
            .into_iter()
            .zip(corrected.into_iter())
            .filter( |(a, b)| a != b)
            .map( |(a, b)| format!("s/{}/{}", a, b))
            .collect()
    }

}

#[async_trait]
trait Net {
    async fn connect(&mut self) -> Result<(), Box<dyn Error>>;
    async fn join(&mut self) -> Result<(), Box<dyn Error>>;
    async fn listen(&mut self) -> Result<(), Box<dyn Error>>;
    async fn send(&mut self, words: &String) -> Result<(), Box<dyn Error>>;
    async fn dispatch(&mut self, cmd: Option<Command>) -> Result<(), Box<dyn Error>>;
}

pub struct Irc<'a> {
    registration: Registration,
    stream: &'a mut TcpStream,
    spell_check: &'a mut SpellCheck
}

pub struct Bot<'a> {
    network: Box<dyn Net + 'a>,
}

impl Bot <'_> {
    fn new(network: Box<dyn Net>) -> Self {
        Bot { network }
    }

    async fn start(&mut self) -> () {
        self.network.connect().await;
        self.network.join().await;
        self.network.listen().await;
    }
}

#[async_trait]
impl Net for Irc <'_> {
    async fn connect(&mut self) -> Result<(), Box<dyn Error>> {
        let registration_str: String = self.registration.create_registration_str();
        self.stream.write_all(&registration_str.as_bytes()).await;
        Ok(())
    }

    async fn join(&mut self) -> Result<(), Box<dyn Error>> {
        let join = format!("JOIN {}\n", CHANNEL);
        self.stream.write_all(join.as_bytes()).await;
        Ok(())
    }

    async fn listen(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            let mut buf = [0; 1024];
            let n = self.stream.read(&mut buf).await?;
            let recieved = String::from_utf8(buf[0..n].to_vec())?;
            println!("recieved: {}", recieved);

            let cmd: Option<Command> = parse_command(recieved.clone());
            self.dispatch(cmd).await;
        }
    }

    async fn send(&mut self, words: &String) -> Result<(), Box<dyn Error>> {
        let send = format!("PRIVMSG #didnt :{}\n", words);
        self.stream.write_all(send.as_bytes()).await?;
        Ok(())
    }

    async fn dispatch(&mut self,
                      cmd: Option<Command>)
                      -> Result<(), Box<dyn Error>>{
        match cmd {
            Some(Command::PING(p)) => {
                let Ping { server: s } = p;
                println!("got pinged from server: {}", s);
                let response = format!("PONG {}\n", s);
                println!("sending pong: {}", response);
                self.stream.write_all(response.as_bytes()).await?;
                Ok(())
            },
            Some(Command::PRIVMSG(m)) => {
                let Message { sender: s, channel: c, text: t } = m;

                let non_static = SCRIBE.clone();
                match s.split("@").nth(0) {
                    Some(non_static) => { 
                        let c = self.spell_check.correct(&t);
                        for w in c {
                            self.send(&w).await;
                        };

                        Ok(())
                    }
                    _ => Ok(())
                }
            },
            _ => Ok(())
        }
    }
}

impl Registration {
    pub fn new() -> Registration {
        let u = User {
            user: String::from("guest"),
            mode: String::from("tolmoon"),
            unused: String::from("tolsun"),
            realname: String::from(":Ronnie Regan")
        };

        let n = Nick {
            nick: String::from(NICK)
        };

        Registration {
            user: u,
            nick: n
        }
    }

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
    let contents = fs::read_to_string("./src/training.txt")
        .expect("Something went wrong reading the file");

    let mut spell_check = SpellCheck::new(contents);
    let mut stream = TcpStream::connect(ADDR).await?;

    let r = Registration::new();
    let mut irc = Irc {
        registration: r,
        stream: &mut stream,
        spell_check: &mut spell_check
    };

    let mut bot = Bot { network: Box::from(irc) };
    bot.start().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::*;
    use mockall::predicate::*;

    #[test]
    fn event_parse_pong() {
        let expected_cmd = 
            Command::PING( Ping { server: "public-irc.w3.org".to_string() } );
        let cmd_str = "PING :public-irc.w3.org\r\n".to_string();
        let cmd = parse_command(cmd_str).unwrap();
        assert_eq!(cmd, expected_cmd);
    }

    #[test]
    fn message_parse_sentance_test() {
        let expected_cmd =
            Command::PRIVMSG( Message { 
                sender: "anubis@public.cloak".to_string(),
                channel: "#didnt".to_string(),
                text: "well hello there".to_string()
            } );

        let message_str = 
            "anubis@public.cloak PRIVMSG #didnt :well hello there\r\n".to_string();

        let message = parse_command(message_str).unwrap();
        assert_eq!(message, expected_cmd);
        //assert_eq!(ch, "#didnt");
    }

    #[test]
    fn spellcheck_sentance_with_a_mistake() {
        let mut spell_check = SpellCheck::new("tomato pizza".to_string());
        
        let expected_text = vec!["s/tomata/tomato"];
        let result = spell_check.correct(&"I like to eat tomata pizza".to_string());
        assert_eq!(result, expected_text);

    }

    #[test]
    fn spellcheck_sentance_with_many_mistakes() {
        let mut spell_check = SpellCheck::new("tomato pizza".to_string());

        let expected_text = vec!["s/tomata/tomato", "s/pzza/pizza"];
        let result = spell_check.correct(&"I like to eat tomata pzza".to_string());
        assert_eq!(result, expected_text);
    }

    #[test]
    fn spellcheck_with_no_mistakes() {
        let mut spell_check = SpellCheck::new("tomato pizza".to_string());

        let result = spell_check.correct(&"I like to eat tomato pizza".to_string());
        let empty_vec: Vec<String> = [].to_vec();
        assert_eq!(result, empty_vec);
    }

    #[test]
    fn spellcheck_does_not_correct_nonscribes() {
        let msg =
            Message { 
                sender: "anubis@public.cloak".to_string(),
                channel: "#didnt".to_string(),
                text: "I like to eat tomata pizza".to_string()

            };

        let cmd = Command::PRIVMSG(msg);
    }
}

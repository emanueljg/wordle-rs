use std::{ascii::AsciiExt, fmt::Display, fs::{self, File, Permissions, read}, io::{BufRead, BufReader, Read, Write}, path::PathBuf, str::FromStr};

use clap::{ArgAction, ColorChoice, Command, Parser, arg, command, value_parser};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, ParseError, Utc};
use serde::Deserialize;
use std::io;
extern crate reqwest;
use colored::Colorize;


extern crate dirs;

static ALPHABET: [char; 26] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm',
    'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z'];

// {"id":1566,"solution":"fable","print_date":"2026-01-01","days_since_launch":1657,"editor":"Tracy Bennett"}
#[derive(Deserialize, Debug)]
struct WordleResponse {
    id: u32,
    solution: String,
    print_date: String,
    days_since_launch: u32,
    editor: String,
}

/// Simple program to greet a person
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(
        short,
        long,
        value_parser = clap::builder::ValueParser::new(parse_naive_date),
        default_value_t = Utc::now().date_naive()
    )]
    day: NaiveDate,

    /// Number of times to greet
    #[arg(short, long, default_value_os_t = dirs::cache_dir().unwrap())]
    cachedir: PathBuf,
}

fn parse_naive_date(date: &str) -> chrono::ParseResult<NaiveDate> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
}


fn unwrap_io_result(e: io::Error, msg: &str) -> ! {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            eprintln!("Error {}: no permission", msg);
            std::process::exit(1);
        },
        _ => {
            eprintln!("Error {}: unknown error", msg);
            std::process::exit(1)
        }
    }
}

fn get_word(args: Args) -> (File, String) {
    let yyyymmdd = args.day.format("%Y-%m-%d").to_string();


    fs::create_dir_all(&args.cachedir).unwrap_or_else(|e| unwrap_io_result(e, "creating cache dir"));

    let word_cache_path = args.cachedir.join(&yyyymmdd);

    match word_cache_path.try_exists() {
        Err(e) => unwrap_io_result(e, "checking for word cache"),
        Ok(false) => { 
            let mut f = File::create_new(word_cache_path).unwrap_or_else(
                |e| unwrap_io_result(e, "creating word cache file")
            );
            let word = reqwest::blocking::get(
                format!(
                    "https://www.nytimes.com/svc/wordle/v2/{}.json"
                    , yyyymmdd
                )
            ).unwrap().json::<WordleResponse>().expect("should serialize").solution;
            f.write(word.as_bytes()).unwrap_or_else(|e| unwrap_io_result(e, "writing to word cache file"));
            (f, word)
        },
        Ok(true) => {
            let f = File::open(word_cache_path).unwrap_or_else(
                |e| unwrap_io_result(e, "opening word cache file")
            );
            let mut r = BufReader::new(&f);
            let mut buf = String::new();
            r.read_line(&mut buf).unwrap_or_else(
                |e| unwrap_io_result(e, "reading word cache file")
            );
            (f, buf.trim_end().to_string())
        },
    }
} 

#[derive(Debug)]
enum CharGuess {
    NotInWord(char),
    WrongPlace(char),
    Correct(char),
}

struct CurrentWord {
    correct_answer: String,
    guesses: Vec<CharGuess>
}

impl CurrentWord {
    fn new(correct_answer: String) -> Self {
        Self { correct_answer, guesses: vec![] }
    }

    fn update_guess(&mut self, guess: String) {
        self.guesses = guess.chars().enumerate().map(
            |(i, ch)| { 
                if self.correct_answer.chars().nth(i).unwrap() == ch {
                    CharGuess::Correct(ch)
                }
                else if self.correct_answer.contains(ch) {
                    CharGuess::WrongPlace(ch)
                } else {
                    CharGuess::NotInWord(ch)
                }
            }
       ).collect() 
    }

    fn display_word(&self) {
        if self.guesses.is_empty() {
            println!("_____");
        } else {
            for cg in &self.guesses {
                match cg {
                    CharGuess::NotInWord(x) => print!("{}", x.to_string().on_bright_black().black()),
                    CharGuess::WrongPlace(x) => print!("{}", x.to_string().on_yellow().black()),
                    CharGuess::Correct(x) => print!("{}", x.to_string().on_green().black()),
                }   
            }
            print!("\n");
        }
    }
}


fn main() {
    let args = Args::parse();
    let (cache_file, answer) = get_word(args);
    let mut current_word = CurrentWord::new(answer);

    loop {
        print!("\n");
        current_word.display_word();
        print!("\n");

        let guess = std::io::stdin()
            .lines()
            .next()
            .unwrap()
            .unwrap()
            .to_ascii_lowercase()
            .trim()
            .to_string();

        // error handling
        if guess.len() < 5 {
            println!("Word can't be less that 5 characters long!");
            continue;
        } else if guess.len() > 5 {
            println!("Word can't be more than 5 characters long!");
            continue;
        } else if guess.chars().any(|ch| !ALPHABET.contains(&ch)) {
            println!("Word can't contain non-letter characters! [a-z]");
            continue;
        }

        current_word.update_guess(guess);


    }

}
// }

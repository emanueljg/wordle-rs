use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    collections::HashSet,
};

use clap::{Parser, arg, command};
use chrono::{Days, NaiveDate,Utc};
use serde::Deserialize;
use reqwest;
use dirs;
use colored::Colorize;


static ALPHABET: [char; 26] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm',
    'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z'];

static DATE_FORMAT: &str = "%Y-%m-%d";

#[derive(Deserialize, Debug)]
#[serde(untagged)]
#[allow(dead_code)]
enum WordleResponse {
    Success {
        id: u32,
        solution: String,
        print_date: String,
        days_since_launch: u32,
        editor: String,
    },

    Failure {
        status: String,
        errors: Vec<String>,
        results: Vec<String>,
    },
}

fn write_dictionary(dict_path: &PathBuf, client: &reqwest::blocking::Client) -> File {
    let f = File::options()
        .read(true)
        .write(true)
        .create(true)
        .open(dict_path)
        .unwrap_or_else(|e| unwrap_io_result(e, "creating dict file"));
    let mut bw = BufWriter::new(f);

    let html = client.get(
        "https://gist.githubusercontent.com/dracos/dd0668f281e685bad51479e5acaadb93/raw/6bfa15d263d6d5b63840a8e5b64e04b382fdb079/valid-wordle-words.txt",
    ).send().unwrap().text().unwrap();

    bw.write(html.as_bytes()).unwrap_or_else(|e| unwrap_io_result(e, "writing dict file"));
    bw.into_inner().unwrap()
}


/// Wordle in Rust.
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The day of the wordle to play
    #[arg(
        short,
        long,
        value_parser = clap::builder::ValueParser::new(parse_naive_date),
        default_value_t = Utc::now().date_naive()
    )]
    day: NaiveDate,

    /// The directory to place data in.
    #[arg(short, long, default_value_os_t = dirs::cache_dir().unwrap().join("wordle-rs"))]
    cache_dir: PathBuf,

    /// Whether to force-update the dictionary
    #[arg(short, long, default_value_t = false)]
    update_dictionary: bool,

    /// Whether to prefetch wordles
    #[arg(short, long, default_value_t = false)]
    prefetch_wordles: bool,
}

fn parse_naive_date(date: &str) -> chrono::ParseResult<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT)
}

fn unwrap_io_result(e: io::Error, msg: &str) -> ! {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            eprintln!("Error {}: no permission", msg);
        },
        _ => {
            eprintln!("Error {}: unknown error ({})", msg, e);
        }
    }
    std::process::exit(1);
}

fn get_and_write_word(cache_dir: &PathBuf, day: NaiveDate, client: &reqwest::blocking::Client) -> Option<(File, String)> {
    let yyyymmdd = day.format(DATE_FORMAT).to_string();

    let word_cache_path = cache_dir.join(&yyyymmdd);

    match word_cache_path.try_exists() {
        Err(e) => unwrap_io_result(e, "checking for word cache"),

        Ok(false) =>  
            match client.get(
                format!("https://www.nytimes.com/svc/wordle/v2/{}.json", yyyymmdd)
            )
            .send()
            .unwrap()
            .json::<WordleResponse>() {
                Ok(WordleResponse::Success { id: _, solution, print_date: _, days_since_launch: _, editor: _ }) => {
                    let mut f = File::create_new(word_cache_path).unwrap_or_else(
                        |e| unwrap_io_result(e, "creating word cache file")
                    );
                    f.write(solution.as_bytes()).unwrap_or_else(|e| unwrap_io_result(e, "writing to word cache file"));
                    Some((f, solution))
                },
                Ok(WordleResponse::Failure { status: _, errors: _, results: _ }) => None,
                Err(e) => {
                    panic!("{:?} {:?} {:?}", e, e.url(), e.status())
                }
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
            Some((f, buf.trim_end().to_string()))
        },
    }
} 

#[derive(Debug)]
enum CharGuessKind {
    NotInWord,
    WrongPlace,
    Correct,
}

struct CharGuess {
    ch: char,
    kind: CharGuessKind
}

impl CharGuess {
    fn new(ch: char, kind: CharGuessKind) -> Self {
        Self { ch, kind }
    }
}

enum InvalidGuessKind {
    WordTooLong,
    WordTooShort,
    WordContainsNonLetters,
    WordNotInDictionary,
}

enum GuessOutcome {
    InvalidGuess(InvalidGuessKind),
    Continue,
    Win,
    NoTriesLeft,
}

struct CurrentWord {
    correct_answer: String,

    char_guesses: Vec<Vec<CharGuess>>,

    tries: u32
}

impl CurrentWord {
    fn new(correct_answer: String, tries: u32) -> Self {
        Self { correct_answer, tries, char_guesses: vec![] }
    }

    fn current_guess(&self) -> String {
        let mut s = String::new();
        for cg in self.char_guesses.iter().rev().next().unwrap() {
            s.push(cg.ch);
        };
        s
    } 

    fn guess(&mut self, guess: String, dictionary: &HashSet<String>) -> GuessOutcome {
        if guess.len() < 5 {
            GuessOutcome::InvalidGuess(InvalidGuessKind::WordTooShort)
        } else if guess.len() > 5 {
            GuessOutcome::InvalidGuess(InvalidGuessKind::WordTooLong)
        } else if guess.chars().any(|ch| !ALPHABET.contains(&ch)) {
            GuessOutcome::InvalidGuess(InvalidGuessKind::WordContainsNonLetters)
        } else if !dictionary.contains(&guess) {
            GuessOutcome::InvalidGuess(InvalidGuessKind::WordNotInDictionary)
        } else {
            self.char_guesses.push(guess.chars().enumerate().map(
                |(i, ch)| { 
                    if self.correct_answer.chars().nth(i).unwrap() == ch {
                        CharGuess::new(ch, CharGuessKind::Correct)
                    }
                    else if self.correct_answer.contains(ch) {
                        CharGuess::new(ch, CharGuessKind::WrongPlace)
                    } else {
                        CharGuess::new(ch, CharGuessKind::NotInWord)
                    }
                }
           ).collect()); 

           self.tries -= 1;

            if self.current_guess() == self.correct_answer {
                GuessOutcome::Win
            } else if self.tries == 0 {
                GuessOutcome::NoTriesLeft
            } else {
                GuessOutcome::Continue
            }
        }
    } 

    fn display_word(&self) {
        if self.char_guesses.is_empty() {
            println!("_____");
        } else {
            for cgs in &self.char_guesses {
                for cg in cgs {
                    match cg.kind {
                        CharGuessKind::NotInWord => print!("{}", cg.ch.to_string().on_bright_black().black()),
                        CharGuessKind::WrongPlace => print!("{}", cg.ch.to_string().on_yellow().black()),
                        CharGuessKind::Correct => print!("{}", cg.ch.to_string().on_green().black()),
                    }   
                }
                println!();
            }
            println!();
        }
    }
}


fn main() {
    let args = Args::parse();

    let client = reqwest::blocking::Client::new();

    fs::create_dir_all(&args.cache_dir).unwrap_or_else(|e| unwrap_io_result(e, "creating cache dir"));

    let dict_path = args.cache_dir.join("dictionary");
    if args.update_dictionary {
        write_dictionary(&dict_path, &client);
        std::process::exit(0);
    };
    let dictionary =
        BufReader::new(
            File::open(&dict_path)
                .unwrap_or_else(|e| match e.kind() {
                    io::ErrorKind::NotFound => write_dictionary(&dict_path, &client),
                    _ => unwrap_io_result(e, "opening dictionary file"),
                })
        )
        .lines()
        .map(
            |res| res.unwrap_or_else(
                |e| unwrap_io_result(e, "reading dictionary word")
            )
        )
        .collect();

    if args.prefetch_wordles {
        let mut current_day = args.day;
        eprintln!("Wordle prefetch requested! Starting from {}.", current_day);
        loop {
            if let Some(_) = get_and_write_word(&args.cache_dir, current_day, &client) {
                eprintln!("{}: Successfully read/fetched the word", current_day);
                current_day = current_day.checked_add_days(Days::new(1)).unwrap();
            } else {
                eprintln!("{}: No word from NYtimes for this date yet. Ending prefetch process here.", current_day);
                break;
            };
        }
        eprintln!("Prefetch done.");
        if current_day == args.day {
            eprintln!("No days were prefetched. This is rare. You probably set a custom --day too far into the future.");
        } else {
            eprintln!("{} days prefetched, {} - {}.",
                (current_day - args.day).num_days(),
                args.day,
                current_day
            );
        }
    }

    if args.prefetch_wordles || args.update_dictionary {
        std::process::exit(0);
    }

    let (_, answer) = get_and_write_word(&args.cache_dir, args.day, &client).unwrap_or_else(
        || {
            eprintln!("Recieved an error response from NYT. This probably means that the day's wordle is not published yet.");
            std::process::exit(1)
        }
    );
    let mut current_word = CurrentWord::new(answer, 5);

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

        match current_word.guess(guess, &dictionary) {
            GuessOutcome::InvalidGuess(InvalidGuessKind::WordTooShort) => 
                println!("Word can't be less that 5 characters long!"),
            GuessOutcome::InvalidGuess(InvalidGuessKind::WordTooLong) => 
                println!("Word can't be more than 5 characters long!"),
            GuessOutcome::InvalidGuess(InvalidGuessKind::WordContainsNonLetters) => 
                println!("Word can't contain non-letter characters! [a-z]"),
            GuessOutcome::InvalidGuess(InvalidGuessKind::WordNotInDictionary) => 
                println!("Word not in dictionary!"),
            GuessOutcome::Continue => (),
            GuessOutcome::Win => {
                current_word.display_word();
                println!("congratz!");
                std::process::exit(0)
            },
            GuessOutcome::NoTriesLeft => {
                current_word.display_word();
                println!("womp womp");
                std::process::exit(0)
            },
        }

    }

}
// }

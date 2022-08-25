use console::Style;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, FuzzySelect, Input, Select};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use reqwest::Client;
use std::cmp::min;
use std::fs::File;
use std::io::Write;

use libgen::api::book::Book;
use libgen::api::download::DownloadRequest;
use libgen::api::mirrors::{Mirror, MirrorList, MirrorType};
use libgen::api::search::{Search, SearchOption};

pub mod cli_args;
use crate::libgen_cli::cli_args::Args;

lazy_static! {
    static ref RED_STYLE: Style = Style::new().red();
}

pub fn parse_mirrors() -> MirrorList {
    let mut config_path = dirs::config_dir().unwrap();
    config_path.push("libgen-rs/mirrors.json");
    let json = std::str::from_utf8(&std::fs::read(config_path).expect("Couldn't read config file"))
        .unwrap()
        .to_owned();
    MirrorList::parse_mirrors(&json)
}

pub fn select_search_mirror(mirrors: &MirrorList) -> Result<Mirror, &'static str> {
    let mirror_selection = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Search mirror")
        .default(0)
        .items(&mirrors.search_mirrors)
        .interact_opt()
        .unwrap();
    mirrors.get(MirrorType::Search, mirror_selection.unwrap())
}

pub fn input_search_request() -> Result<String, &'static str> {
    Ok(Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Search request")
        .interact_text()
        .expect("You must specify a request"))
}

pub fn input_search_option() -> Result<SearchOption, &'static str> {
    let selections = &[
        "Default",
        "Title",
        "Author",
        "Series",
        "Publisher",
        "Year",
        "Identifier",
        "Language",
        "MD5",
        "Tags",
        "Extension",
    ];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Search option")
        .default(0)
        .items(&selections[..])
        .interact()
        .unwrap();
    match selection {
        0 => Ok(SearchOption::Default),
        1 => Ok(SearchOption::Title),
        2 => Ok(SearchOption::Author),
        3 => Ok(SearchOption::Series),
        4 => Ok(SearchOption::Publisher),
        5 => Ok(SearchOption::Year),
        6 => Ok(SearchOption::ISBN),
        7 => Ok(SearchOption::Language),
        8 => Ok(SearchOption::MD5),
        9 => Ok(SearchOption::Tags),
        10 => Ok(SearchOption::Extension),
        _ => Err("Unknown option"),
    }
}

pub fn input_results_count() -> Result<u32, &'static str> {
    let selections = &[25, 50, 100];

    Ok(selections[Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Results per request")
        .default(0)
        .items(&selections[..])
        .interact()
        .unwrap()])
}

pub fn fuzzyselect_book(books: &[Book]) -> Result<Book, &'static str> {
    let book = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select book")
        .default(0)
        .items(books)
        .interact_opt()
        .unwrap();
    Ok(books.get(book.expect("Book not selected")).unwrap().clone())
}

pub fn print_book_info(book: &Book) -> Result<(), &'static str> {
    println!("{}: {}", RED_STYLE.apply_to("ID"), book.id);
    println!("{}: {}", RED_STYLE.apply_to("Title"), book.title);
    println!("{}: {}", RED_STYLE.apply_to("Author"), book.author);
    println!(
        "{}: {:.2} Mb",
        RED_STYLE.apply_to("Filesize"),
        book.filesize.parse::<u32>().unwrap() as f32 / 1048576.0
    );
    println!("{}: {}", RED_STYLE.apply_to("Year"), book.year);
    println!("{}: {}", RED_STYLE.apply_to("Language"), book.language);
    println!("{}: {}", RED_STYLE.apply_to("Pages"), book.pages);
    println!("{}: {}", RED_STYLE.apply_to("Publisher"), book.publisher);
    println!("{}: {}", RED_STYLE.apply_to("Edition"), book.edition);
    println!("{}: {}", RED_STYLE.apply_to("MD5"), book.md5);
    println!("{}: {}", RED_STYLE.apply_to("Cover"), book.coverurl);
    Ok(())
}

pub fn select_download_mirror(mirrors: &MirrorList) -> Result<Mirror, &'static str> {
    let mirror_selection = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Download mirror")
        .default(0)
        .items(&mirrors.download_mirrors)
        .interact_opt()
        .unwrap();
    mirrors.get(MirrorType::Download, mirror_selection.unwrap())
}

async fn search_loop(
    args: &Args,
    mirrors: &MirrorList,
    client: &Client,
) -> Result<Vec<Book>, &'static str> {
    let search_mirror = match select_search_mirror(mirrors) {
        Ok(mirror) => mirror,
        Err(_) => return Err("You must select a mirror"),
    };
    let request = match &args.search {
        Some(req) => String::from(req),
        None => input_search_request().expect("Empty request"),
    };
    let search_option = input_search_option().unwrap();
    let results = input_results_count().unwrap();
    let search_options: Search = Search {
        mirror: search_mirror.clone(),
        request,
        results,
        search_option,
    };
    println!("Search at {}... This may take a while", search_mirror);
    let received_books = search_options.search(client).await?;
    if received_books.is_empty() {
        Err("Books not found")
    } else {
        Ok(received_books)
    }
}

async fn download_loop(
    mirrors: &MirrorList,
    books: &[Book],
    client: &Client,
) -> Result<bool, &'static str> {
    let selected_book = fuzzyselect_book(books).expect("Empty book");
    print_book_info(&selected_book).unwrap();
    if !Confirm::new()
        .with_prompt("Do you want to download this book?")
        .interact()
        .unwrap()
    {
        return Ok(false);
    }
    let download_mirror = select_download_mirror(mirrors).unwrap();
    let download_request = DownloadRequest {
        mirror: download_mirror,
    };
    let down_req = download_request
        .download_book(client, &selected_book)
        .await?;
    let total_size = down_req.content_length().unwrap();

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .unwrap()
        .progress_chars("#>-"));
    pb.set_message("Downloading...");

    let mut book_download_path = dirs::download_dir().unwrap();
    book_download_path.push("libgen-rs");
    std::fs::create_dir_all(&book_download_path).unwrap();
    if selected_book.title.len() >= 249 {
        book_download_path.push(&selected_book.title[0..249]);
    } else {
        book_download_path.push(&selected_book.title);
    }
    book_download_path.set_extension(&selected_book.extension);
    let mut stream = down_req.bytes_stream();
    let mut file = File::create(book_download_path).unwrap();
    let mut downloaded: u64 = 0;
    while let Some(item) = stream.next().await {
        let chunk = item.or(Err("Error while downloading file")).unwrap();
        file.write_all(&chunk).unwrap();
        let new = min(downloaded + (chunk.len() as u64), total_size);
        downloaded = new;
        pb.set_position(new);
    }
    Ok(true)
}

async fn download_book_from_md5(
    mirrors: &MirrorList,
    client: &Client,
    md5: String,
) -> Result<(), &'static str> {
    let search_mirror = match select_search_mirror(mirrors) {
        Ok(mirror) => mirror,
        Err(_) => return Err("You must select a mirror"),
    };
    let search_options: Search = Search {
        mirror: search_mirror.clone(),
        request: md5,
        results: 25,
        search_option: SearchOption::MD5,
    };
    println!("Search at {}... This may take a while", search_mirror);
    let received_books = search_options.search(client).await?;
    if received_books.is_empty() {
        return Err("Books not found");
    } else {
        download_loop(mirrors, &received_books, client).await?;
    }
    Ok(())
}

pub async fn init(args: &Args) -> Result<(), &'static str> {
    let client = Client::new();
    let mirrors = parse_mirrors();

    if args.download.is_some() {
        download_book_from_md5(
            &mirrors,
            &client,
            String::from(args.download.as_ref().unwrap()),
        )
        .await
        .unwrap();
        return Ok(());
    }

    let books = loop {
        match search_loop(args, &mirrors, &client).await {
            Ok(books) => break books,
            Err(err) => {
                if args.search.is_none() {
                    continue;
                } else {
                    return Err(err);
                }
            }
        }
    };

    loop {
        match download_loop(&mirrors, &books, &client).await {
            Ok(v) => {
                if v {
                    break;
                } else {
                    continue;
                }
            }
            Err(err) => return Err(err),
        }
    }

    Ok(())
}

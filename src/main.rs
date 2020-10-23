//use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind, Read, Write};
use std::mem;

//use futures::future::join_all;
//use futures::stream::{self, StreamExt};
//use itertools::Itertools;
use select::document::Document;
use select::predicate::{Child, Class, Name, Predicate};

type CrawlerResult<T> = Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() {
    if let Err(err) = _main().await {
        eprintln!("{}", err);
    }
}

async fn _main() -> CrawlerResult<()> {
    // Fetch the index of the "Official SecurityMonkey Case File Index".
    let body = get_page_cached("https://web.archive.org/web/20140101090842/http://it.toolbox.com/blogs/securitymonkey/official-securitymonkey-case-file-index-14787", "raw/index.html").await?;
    //println!("body = {:?}", body);
    let document = Document::from(body.as_ref());

    let cases = get_cases(&document);
    //println!("{:?}", cases);

    let tasks = cases
        .into_iter()
        .map(|(case, sections)| {
            (case, sections.into_iter().map(|(section, link)| (section, link.to_owned())).collect())
        })
        .map(|(case, sections): (String, Vec<(String, String)>)| {
            let case = case.clone();
            tokio::spawn(async move {
                let mut case_text = format!("<h1>{}</h1>\n", case);
                for (section, link) in sections {
                    case_text = case_text + "<h2>" + &section + "</h2>\n";
                    let filename = format!("raw/{}_{}", case, section);
                    let body = get_page_cached(&link, &filename).await.expect("failed to fetch page");
                    //println!("{:?}: {:#?}", filename, body);
                    let document = Document::from(body.as_ref());
                    let section_text = get_section_text(&document).unwrap();
                    case_text = case_text + &section_text;
                }
                case_text + "\n"
            })
        })
        .collect::<Vec<_>>();
    let mut case_texts = Vec::new();
    for task in tasks {
        case_texts.push(task.await.unwrap());
    }

    let filename = format!("out/security_monkey_case_files.html");
    let file = File::create(filename).unwrap();
    let mut buf_writer = BufWriter::new(file);
    for case_text in case_texts {
        buf_writer.write_all(case_text.as_bytes()).unwrap();
    }
    //println!("{:?}: {}", filename, section_text);

    Ok(())
}

async fn get_page_cached(url: &str, filename: &str) -> CrawlerResult<String> {
    // Try to open a new file for writing the cached response into.
    let file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(filename) {
        // If the file already exists, we re-use the cached copy.
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            let file = File::open(filename)?;
            let mut buf_reader = BufReader::new(file);
            let mut contents = String::new();
            buf_reader.read_to_string(&mut contents)?;
            return Ok(contents);
        },
        // Treat other errors as unrecoverable.
        Err(err) => return Err(Box::new(err)),
        Ok(file) => file,
    };
    // If we successfully created a new file, fetch the page and write
    // the body of the request into it.
    let body = reqwest::get(url)
        .await?
        .text()
        .await?;
    let mut buf_writer = BufWriter::new(file);
    buf_writer.write_all(body.as_bytes())?;
    Ok(body)
}

type SectionLink<'a> = (String, &'a str);
type SectionName = String;
type Section<'a> = (SectionName, Vec<SectionLink<'a>>);
type Cases<'a> = Vec<Section<'a>>;

// Filter the page into just the links to the case files.
fn get_cases(document: &Document) -> Cases {
    // The links are all the `a` elements in the `blog-content` div, up
    // until an hrule, after which there's a footer.
    let elements = document.find(
        Class("blog-content")
            .descendant(
                Name("a")
                    .or(Name("h1"))
                    .or(Name("hr"))));
    // Skip the first element since it's an hrule.
    // Skip elements after the second hrule since they're part of the
    // footer.
    let mut elements = elements.skip(1).take_while(|node| node.name() != Some("hr"));

    // We now have an iterator of links separated by headers, which we
    // group into cases which consist of sections, with each section
    // being a link to a section of the case.
    let mut cases = Vec::new();
    let mut case = elements.next().unwrap().text();
    let mut sections = Vec::new();
    for element in elements {
        if let Some("h1") = element.name() {
            cases.push((case, mem::take(&mut sections)));
            case = element.text();
            continue;
        }

        assert_eq!(element.name(), Some("a"));
        sections.push((element.text(), element.attr("href").unwrap()));
    }

    cases
}

struct SkipLast<I: Iterator> {
    iter: I,
    next: Option<I::Item>,
}

impl<I: Iterator> SkipLast<I> {
    fn new(mut iter: I) -> SkipLast<I> {
        let next = iter.next();
        SkipLast { iter, next }
    }
}

impl<I: Iterator> Iterator for SkipLast<I> {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        match self.iter.next() {
            None => None,
            peeked => mem::replace(&mut self.next, peeked),
        }
    }
}

fn get_section_text(document: &Document) -> CrawlerResult<String> {
    let elements = document
        .find(Class("blog-content").or(Class("blogs_entrybody")))
        .map(
            |post| SkipLast::new(post
                .children()
                .take_while(|element| !element
                    .children()
                    .any(|child| child.is(Child(Name("h3"), Name("a"))))
                ))
                .take_while(|element| !element.is(Name("table")))
                .take_while(|element| !element
                    .descendants()
                    .any(|descendant| descendant.is(Child(Name("a"), Name("img"))))
                )
                .take_while(|element| !(element.is(Name("b")) && element.text().contains("Continued in PART")))
                .take_while(|element| !element.text().contains("ENTIRE case file index"))
                .take_while(|element| !element.text().contains("Read all of my case files"))
                .map(|element| element.html())
                .collect::<Vec<_>>()
                .join("\n")
        )
        .collect::<Vec<_>>()
        .join("\n");
    Ok(elements)
}


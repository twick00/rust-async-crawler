use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use std::borrow::Borrow;
use url::{ParseError, Url};

use async_std::sync::Sender;
use async_std::{sync, task};
use std::str::FromStr;
use surf;

type CrawlResult = Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
type BoxFuture = std::pin::Pin<Box<dyn std::future::Future<Output = CrawlResult> + Send>>;

#[derive(Default, Debug)]
struct LinkQueue {
    links: Vec<String>,
}
impl TokenSink for &mut LinkQueue {
    type Handle = ();
    fn process_token(&mut self, token: Token, line_number: u64) -> TokenSinkResult<Self::Handle> {
        match token {
            Token::TagToken(
                ref tag @ Tag {
                    kind: TagKind::StartTag,
                    ..
                },
            ) => {
                if tag.name.as_ref() == "a" {
                    for attribute in tag.attrs.iter() {
                        if &attribute.name.local == "href" {
                            let url_str: &[u8] = attribute.value.borrow();
                            self.links
                                .push(String::from_utf8_lossy(url_str).into_owned())
                        }
                    }
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

pub fn get_links(url: &Url, page: String) -> Vec<Url> {
    let mut domain_url = url.clone();
    domain_url.set_path("");
    domain_url.set_query(None);

    let mut queue = LinkQueue::default();
    let mut tokenizer = Tokenizer::new(&mut queue, TokenizerOpts::default());
    let mut buffer = BufferQueue::new();
    buffer.push_back(page.into());
    let _ = tokenizer.feed(&mut buffer);

    queue
        .links
        .iter()
        .map(|link| match Url::parse(link) {
            Err(ParseError::RelativeUrlWithoutBase) => domain_url.join(link).unwrap(),
            Err(_) => panic!("Malformed link found: {}", link),
            Ok(url) => url,
        })
        .collect()
}

fn box_crawl(pages: Vec<Url>, sender: Sender<String>, current: u8, max: u8) -> BoxFuture {
    Box::pin(crawl(pages, sender, current, max))
}

async fn crawl(pages: Vec<Url>, sender: Sender<String>, current: u8, max: u8) -> CrawlResult {
    println!("Current Depth: {}, Max Depth: {}", current, max);

    if current > max {
        println!("Reached Max Depth");
        return Ok(());
    }

    let mut tasks = vec![];

    println!("crawling: {:?}", pages);

    for url in pages {
        let sender_clone = sender.clone();
        let task = task::spawn(async move {
            println!("getting: {}", url);

            let mut res = surf::get(&url).await?;
            let body = res.body_string().await?;
            let links = get_links(&url, body);
            for link in &links {
                let t = String::from(link.as_str());
                sender_clone.send(t).await;
            };

            println!("Following: {:?}", links);
            box_crawl(links, sender_clone, current + 1, max).await
        });
        tasks.push(task)
    }

    for task in tasks.into_iter() {
        task.await?;
    }

    Ok(())
}

// fn crawl_url<T: ToString>(url: T, depth: u8) {
//     let s = vec![Url::parse(url.to_string().as_str()).unwrap()];
//     task::block_on(async {
//         box_crawl(s, 1, depth).await
//     });
// }

fn main() {
    // let mut url = std::env::args();
    // let mut url = url.find(|arg| Url::from_str(arg).is_ok()).unwrap();
    let url = String::from("https://www.crawler-test.com/");

    let url = match Url::from_str(&url) {
        Err(ParseError::RelativeUrlWithoutBase) => {
            panic!("A complete url is required. Example:     https://github.com/about")
        }
        Err(e) => panic!(e),
        Ok(url) => url,
    };

    let (s, r) = sync::channel::<String>(1);

    task::spawn(async move {
        let mut counter = 0;
        loop {
            let output = match r.recv().await {
                Some(s) => {
                    println!("Count: {}, {}", counter, s);
                    counter+= 1;
                }
                None => {}
            };
        }
    });

    println!("Url Found: {} \nStarting crawler", &url);
    task::block_on(async { box_crawl(vec![url], s, 1, 1).await });
}

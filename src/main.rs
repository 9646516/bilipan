#![feature(async_closure)]

mod utils;

use clap::{App, Arg};
use reqwest::Client;
use std::io::{BufWriter, Read};
use std::path::Path;
use std::result::Result::Ok;
use utils::Result;

const CFG_PATH: &str = "bilipan.json";

async fn get_opener() -> Result<Client> {
    let mut fs = std::fs::File::open(CFG_PATH)?;
    let mut sb = String::new();
    fs.read_to_string(&mut sb)?;
    let dp = json::parse(&sb)?;
    let dede_user_id = dp["dede_user_id"].as_str().unwrap_or("");
    let dede_user_id_ck_md5 = dp["dede_user_id_ck_md5"].as_str().unwrap_or("");
    let sessdata = dp["sessdata"].as_str().unwrap_or("");
    let bili_jct = dp["bili_jct"].as_str().unwrap_or("");
    if dede_user_id.is_empty()
        || dede_user_id_ck_md5.is_empty()
        || sessdata.is_empty()
        || bili_jct.is_empty()
    {
        Err("".into())
    } else {
        let ret = utils::create_opener(dede_user_id, dede_user_id_ck_md5, sessdata, bili_jct)?;
        Ok(ret)
    }
}

async fn proc() -> Result<Client> {
    return if let Ok(mut opener) = get_opener().await {
        let res = utils::get_user_info(&mut opener).await?;
        let dp = json::parse(&res)?;
        if let Some(res) = dp["data"]["uname"].as_str() {
            println!("Login as {}", res);
            Ok(opener)
        } else {
            println!("login failed, please retry");
            Err("".into())
        }
    } else {
        println!("please login first");
        Err("".into())
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    let start = chrono::offset::Local::now();
    let matches = App::new("Bili Pan")
        .version("0.1.0")
        .arg(
            Arg::new("login")
                .short('l')
                .long("login")
                .about("Login with cookies"),
        )
        .arg(
            Arg::new("upload")
                .about("Upload file")
                .long("upload")
                .short('u')
                .takes_value(true),
        )
        .arg(
            Arg::new("download")
                .about("Download file")
                .long("download")
                .short('d')
                .takes_value(true),
        )
        .get_matches();
    let mut show_time = false;
    let mut sb = String::new();
    if matches.is_present("login") {
        let do_login: bool = {
            let f = std::fs::File::open(CFG_PATH);
            if let Ok(mut x) = f {
                x.read_to_string(&mut sb)?;
                if let Ok(dp) = json::parse(&sb) {
                    let username = dp["username"].as_str().unwrap_or("");
                    if username.is_empty() {
                        true
                    } else {
                        println!("{} has already logined in, override? [Y/N]", username);
                        loop {
                            utils::get_input(&mut sb)?;
                            if sb.len() == 1 {
                                let ch = sb.chars().next().unwrap_or('$');
                                if ch == 'Y' || ch == 'y' {
                                    break true;
                                } else if ch == 'N' || ch == 'n' {
                                    println!("Welcome {}", username);
                                    break false;
                                }
                            }
                            println!("please input [Y/N]");
                        }
                    }
                } else {
                    true
                }
            } else {
                true
            }
        };
        if do_login {
            println!("please input your dede_user_id, dede_user_id_ck_md5, sessdata, bili_jct.Split by a blank space, eg `AAAA BBBB CCCC DDDD`");
            utils::get_input(&mut sb)?;
            let split = sb.split(' ').collect::<Vec<&str>>();
            let mut opener = utils::create_opener(split[0], split[1], split[2], split[3])?;
            let res = utils::get_user_info(&mut opener).await?;
            let dp = json::parse(&res)?;
            if let Some(res) = dp["data"]["uname"].as_str() {
                println!("Login success");
                println!("Welcome {}", res);
                let mut file = BufWriter::new(std::fs::File::create(CFG_PATH)?);
                let u: json::JsonValue = json::object! {
                    "dede_user_id": split[0],
                    "dede_user_id_ck_md5": split[1],
                    "sessdata": split[2],
                    "bili_jct": split[3],
                    "username": res
                };
                u.write(&mut file)?;
            } else {
                println!("Login failed, {}", dp["message"].as_str().unwrap_or(""));
            }
        }
    } else if let Some(c) = matches.value_of("upload") {
        let p = Path::new(c);
        if !p.exists() {
            println!("{} not exists!", c);
        } else if let Ok(mut opener) = proc().await {
            let xor_key = rand::random::<u8>();
            println!("uploading");
            let d = utils::upload_batch(&mut opener, xor_key, p).await?;
            println!("upload part successfully");
            let aes_key = (0..32).map(|_| rand::random::<u8>()).collect::<Vec<u8>>();
            println!("aes_key {:?}", aes_key);
            let buf = utils::generate_idx(p, xor_key, &aes_key, &d)?;
            let idx_url = utils::upload_single(&mut opener, buf).await?;
            println!("idx url {}", idx_url);
            let link = utils::combine(aes_key, idx_url)?;
            println!("upload idx successfully");
            println!("link = {}", link);
            show_time = true;
        }
    } else if let Some(c) = matches.value_of("download") {
        let (aes_key, url) = utils::split(c)?;
        println!("aes_key {:?}", aes_key);
        let idx_url = format!("http://i0.hdslb.com/bfs/album/{}.png", url);
        let (xor_key, filename, sz, urls) = utils::get_index(&idx_url, &aes_key).await?;
        println!("xor_key {}", xor_key);
        println!("filename {}", filename);
        println!("size {} bytes", sz);
        for i in urls.iter() {
            println!("{}", i);
        }
        utils::download_batch(&urls, xor_key, filename, sz).await?;
        println!("done");
        show_time = true;
    } else {
        println!("For more information try `-h`");
    }
    if show_time {
        let end = chrono::offset::Local::now();
        let offet = end - start;
        println!(
            "Elapsed {} secs",
            offet.num_milliseconds() as f32 / 1000.0f32
        );
    }

    Ok(())
}

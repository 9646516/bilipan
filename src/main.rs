#![feature(async_closure)]

mod utils;

use clap::{App, Arg};
use reqwest::Client;
use std::io::{BufWriter, Read};
use std::path::Path;
use std::result::Result::Ok;
use utils::Result;

const CFG_PATH: &str = "./bilipan.json";
const TEMP_PATH: &str = "./TEMP";

fn get_input(sb: &mut String) -> Result<()> {
    sb.clear();
    loop {
        std::io::stdin().read_line(sb)?;
        *sb = sb.trim().to_owned();
        if !sb.is_empty() {
            break;
        }
    }
    Ok(())
}

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

const CHR: &str = "0123456789abcdef";

fn combine(aes_key: Vec<u8>, idx_url: String) -> Result<String> {
    let url = regex::Regex::new("i0.hdslb.com/bfs/album/(.*?).png")?
        .captures_iter(&idx_url)
        .next()
        .ok_or("")?
        .get(1)
        .ok_or("")?
        .as_str()
        .to_string();
    let mut ret = String::with_capacity(40 + aes_key.len());
    for i in &aes_key {
        let hi = ((*i) >> 4) & 0xF;
        let lo = (*i) & 0xF;
        ret.push(CHR.chars().nth(hi as usize).unwrap());
        ret.push(CHR.chars().nth(lo as usize).unwrap());
    }
    ret.push_str(&url);
    Ok(ret)
}

fn split(src: &str) -> Result<(Vec<u8>, String)> {
    let (a, b) = src.split_at(64);
    let mut fst = Vec::new();
    let snd = b.to_owned();
    for i in 0..32 {
        let hi = u8::from_str_radix(&a[(i + i)..(i + i + 1)], 16)?;
        let lo = u8::from_str_radix(&a[(i + i + 1)..(i + i + 1 + 1)], 16)?;
        fst.push(hi << 4 | lo);
    }
    Ok((fst, snd))
}

#[tokio::main]
async fn main() -> Result<()> {
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
                            get_input(&mut sb)?;
                            if sb.len() == 1
                                && (sb.chars().next().unwrap_or('$') == 'Y'
                                || sb.chars().next().unwrap_or('$') == 'y')
                            {
                                break true;
                            } else if sb.len() == 1
                                && (sb.chars().next().unwrap_or('$') == 'N'
                                || sb.chars().next().unwrap_or('$') == 'n')
                            {
                                println!("Welcome {}", username);
                                break false;
                            } else {
                                println!("please input [Y/N]");
                            }
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
            get_input(&mut sb)?;
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
        let p = Path::new(TEMP_PATH);
        if !p.exists() {
            std::fs::create_dir(p)?;
        }
        let p = Path::new(c);
        if !p.exists() {
            println!("{} not exists!", c);
        } else if let Ok(mut opener) = proc().await {
            let xor_key = rand::random::<u8>();
            println!("uploading");
            let d = utils::upload_batch(&mut opener, xor_key, p).await?;

            let idx_path = format!("{}/idx", TEMP_PATH);
            let idx_path = Path::new(&idx_path);

            let aes_key = (0..32).map(|_| rand::random::<u8>()).collect::<Vec<u8>>();
            println!("aes_key {:?}", aes_key);
            utils::generate_idx(p, idx_path, xor_key, &aes_key, &d)?;
            let idx_url = utils::upload_single(&mut opener, idx_path).await?;
            println!("idx url {}", idx_url);
            let link = combine(aes_key, idx_url)?;
            println!("upload successfully");
            println!("link = {}", link);
        }
    } else if let Some(c) = matches.value_of("download") {
        if let Ok(mut opener) = proc().await {
            let (aes_key, url) = split(c)?;
            println!("aes_key {:?}", aes_key);
            let p = Path::new(TEMP_PATH);
            if !p.exists() {
                std::fs::create_dir(p)?;
            }
            let idx_url = format!("http://i0.hdslb.com/bfs/album/{}.png", url);
            let idx_path = format!("{}/idx", TEMP_PATH);
            let idx_path = Path::new(&idx_path);
            utils::download_single(&mut opener, &idx_url, idx_path).await?;
            let (xor_key, filename, sz, idx) = utils::decrypt_aes_single(idx_path, &aes_key)?;
            utils::download_batch(&idx, TEMP_PATH, xor_key, filename, sz).await?;
            println!("done");
        }
    } else {
        println!("For more information try `-h`");
    }
    Ok(())
}

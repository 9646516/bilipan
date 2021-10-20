use std::io::{SeekFrom, Write};
use super::Result;
use reqwest::{header};
use std::path::Path;
use futures::{StreamExt};
use tokio_util::codec::{BytesCodec, FramedRead};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};

pub fn create_opener(
    dede_user_id: &str,
    dede_user_id_ck_md5: &str,
    sessdata: &str,
    bili_jct: &str,
) -> Result<reqwest::Client> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_str("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/88.0.4324.182 Safari/537.36")?,
    );
    let packed = format!(
        "DedeUserID={};DedeUserID__ckMd5={};SESSDATA={};bili_jct={}",
        dede_user_id, dede_user_id_ck_md5, sessdata, bili_jct
    );
    headers.insert(header::COOKIE, header::HeaderValue::from_str(&packed)?);

    Ok(reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .build()?)
}

pub async fn get_user_info(opener: &mut reqwest::Client) -> Result<String> {
    let res = opener
        .get("http://api.bilibili.com/x/web-interface/nav")
        .send()
        .await?;
    Ok(res.text().await?)
}

pub async fn upload_single(opener: &mut reqwest::Client, path: &Path) -> Result<String> {
    let file = tokio::fs::File::open(path).await?;
    let stream = FramedRead::new(file, BytesCodec::new());
    let part1 =
        reqwest::multipart::Part::stream(reqwest::Body::wrap_stream(stream)).file_name("smjb.png");
    let form = reqwest::multipart::Form::new()
        .part("file_up", part1)
        .text("category", "daily");
    let res = opener
        .post("http://api.vc.bilibili.com/api/v1/drawImage/upload")
        .multipart(form)
        .send()
        .await?;
    let text = res.text().await?;
    let dp = json::parse(&text)?;
    Ok(dp["data"]["image_url"].as_str().ok_or("")?.to_string())
}

pub async fn upload_batch(
    base_opener: &mut reqwest::Client,
    xor_key: u8,
    source_path: &Path,
) -> Result<Vec<String>> {
    let sz = source_path.metadata()?.len();
    let buf_size = 1024 * 1024 * 10;
    let epoch = (sz / buf_size) + (if sz % buf_size != 0 { 1 } else { 0 });
    let stream = futures::stream::iter(
        (0..epoch).map(|epoch_id| {
            let opener = base_opener.clone();
            async move {
                let f = tokio::fs::File::open(source_path).await?;
                let mut f = BufReader::new(f);
                f.seek(SeekFrom::Start(epoch_id * buf_size)).await?;
                let mut buf = if epoch_id == epoch - 1 {
                    let mut buf = Vec::new();
                    f.read_to_end(&mut buf).await?;
                    buf
                } else {
                    let mut buf = vec![0u8; buf_size as usize];
                    f.read_exact(&mut buf).await?;
                    buf
                };
                println!("{} block size = {} bytes", epoch_id, buf.len());
                for i in buf.iter_mut() {
                    *i ^= xor_key;
                }
                let buf = super::encode(&mut buf)?;
                let file = std::io::Cursor::new(buf);
                for trys in 0..10 {
                    let res = {
                        let file2 = file.clone();
                        let stream = FramedRead::new(file2, BytesCodec::new());
                        let part1 = reqwest::multipart::Part::stream(reqwest::Body::wrap_stream(stream))
                            .file_name("smjb.png");
                        let form = reqwest::multipart::Form::new()
                            .part("file_up", part1)
                            .text("category", "daily");
                        let future = opener
                            .post("http://api.vc.bilibili.com/api/v1/drawImage/upload")
                            .multipart(form)
                            .send();
                        let s = future.await?;
                        let ret = s.text().await?;
                        let dp = json::parse(&ret)?;
                        if dp["code"].as_i32().ok_or("")? != 0 {
                            Result::Err("bili api failed".into())
                        } else {
                            Result::Ok(dp)
                        }
                    };
                    match res {
                        Ok(x) => {
                            return Result::Ok(x);
                        }
                        Err(x) => {
                            println!("block {} failed,retry [{}/10],message {:?}", epoch_id, trys + 1, x);
                        }
                    }
                }
                Result::Err(format!("block {} failed 10 time", epoch_id).into())
            }
        })
    ).buffered(50);
    let d3 = stream.collect::<Vec<_>>().await;
    let mut ret = Vec::new();
    for i in d3 {
        let dp = i?;
        let url = dp["data"]["image_url"].as_str().ok_or("")?.to_string();
        ret.push(url);
    }
    Ok(ret)
}

pub async fn download_single(opener: &mut reqwest::Client, url: &str, dst: &Path) -> Result<()> {
    let resp = opener.get(url).send().await?;
    let text = resp.bytes().await?;
    let mut out = std::fs::File::create(dst)?;
    out.write_all(&text)?;
    Ok(())
}

async fn try_once(url: &str) -> Result<Vec<u8>> {
    let ret = reqwest::get(url).await?;
    let text = ret.bytes().await?.to_vec();
    Ok(text)
}

async fn get_single(idx: usize, url: &str) -> Result<Vec<u8>> {
    for trys in 0..10 {
        let res = try_once(url).await;
        match res {
            Err(x) => {
                println!("block {} failed,retry [{}/10],message {:?}", idx, trys + 1, x);
            }
            Ok(x) => {
                return Ok(x);
            }
        }
    }
    Result::Err(format!("block {} failed 10 time", idx).into())
}

pub async fn download_batch(
    urls: &[String],
    base: &str, xor_key: u8, filename: String, file_size: u64,
) -> Result<()> {
    for i in urls { println!("{}", i); }
    let sz = urls.len();
    let buf_size = 1024 * 1024 * 10;
    let last_size = file_size - (sz as u64 - 1) * buf_size;
    let dst = format!("{}/{}", base, filename);

    let stream = futures::stream::iter(
        urls.iter().enumerate().map(async move |(idx, x)| {
            let p = format!("{}/{}.part", base, idx);
            let f = tokio::fs::File::create(&p).await?;
            let mut f = tokio::io::BufWriter::new(f);

            let text = get_single(idx, &x).await?;

            let mut buf = super::decode(&text)?;
            if idx == sz - 1 {
                buf.resize(last_size as usize, 0);
            }
            for i in buf.iter_mut() {
                *i ^= xor_key;
            }
            println!("{} block size = {} bytes", idx, buf.len());
            f.write_all(&buf).await?;
            f.flush().await?;
            Result::Ok(())
        })
    ).buffer_unordered(50);
    let res = stream.collect::<Vec<_>>().await;
    for i in res {
        let _ = i?;
    }
    println!("part download finished");
    println!("merge to {}", dst);
    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(dst).await?);
    let mut stream = futures::stream::iter(
        (0..sz).map(async move |idx| {
            let p = format!("{}/{}.part", base, idx);
            let f = tokio::fs::File::open(&p).await?;
            let mut reader = tokio::io::BufReader::new(f);
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await?;
            Result::Ok(buf)
        })
    ).buffered(50);
    while let Some(x) = stream.next().await {
        let buf = x?;
        writer.write_all(&buf).await?;
    }
    writer.flush().await?;
    Ok(())
}

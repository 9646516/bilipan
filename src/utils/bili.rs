use super::Result;
use futures::StreamExt;
use reqwest::header;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, SeekFrom};
use tokio_util::codec::{BytesCodec, FramedRead};

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
        .get("https://api.bilibili.com/x/web-interface/nav")
        .send()
        .await?;
    Ok(res.text().await?)
}

pub async fn upload_index(opener: &mut reqwest::Client, src: Vec<u8>) -> Result<String> {
    let file = std::io::Cursor::new(src);
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

async fn upload_once(file2: std::io::Cursor<Vec<u8>>, opener: &reqwest::Client) -> Result<json::JsonValue> {
    let stream = FramedRead::new(file2, BytesCodec::new());
    let part1 =
        reqwest::multipart::Part::stream(reqwest::Body::wrap_stream(stream))
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
}

async fn upload_single(buf: Vec<u8>, opener: reqwest::Client, epoch_id: u64) -> Result<json::JsonValue> {
    let file = std::io::Cursor::new(buf);
    for trys in 0..10 {
        let res = upload_once(file.clone(), &opener).await;
        match res {
            Ok(x) => {
                return Result::Ok(x);
            }
            Err(x) => {
                println!(
                    "block {} failed,retry [{}/10],message {:?}",
                    epoch_id,
                    trys + 1,
                    x
                );
            }
        }
    }
    Result::Err(format!("block {} failed 10 time", epoch_id).into())
}

pub async fn upload_batch(
    base_opener: &mut reqwest::Client,
    xor_key: u8,
    source_path: &Path,
) -> Result<Vec<String>> {
    let sz = source_path.metadata()?.len();
    let buf_size = 1024 * 1024 * 10;
    let epoch = (sz / buf_size) + (if sz % buf_size != 0 { 1 } else { 0 });
    let mut stream = futures::stream::iter((0..epoch).map(|epoch_id| {
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
            upload_single(buf, opener, epoch_id).await
        }
    }))
        .buffered(15);

    let mut ret = Vec::new();
    while let Some(x) = stream.next().await {
        let dp = x?;
        let url = dp["data"]["image_url"].as_str().ok_or("")?.to_string();
        ret.push(url);
    }
    Ok(ret)
}

pub async fn download_index(url: &str, key: &[u8]) -> Result<(u8, String, u64, Vec<String>)> {
    let resp = reqwest::get(url).await?;
    let text = resp.bytes().await?;
    let buf = text.to_vec();
    let res = super::decrypt_aes_single(buf, key)?;
    Ok(res)
}

async fn download_once(opener: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let ret = opener.get(url).send().await?;
    let text = ret.bytes().await?.to_vec();
    Ok(text)
}

async fn download_single(opener: reqwest::Client, idx: usize, url: &str) -> Result<Vec<u8>> {
    for trys in 0..10 {
        let res = download_once(&opener, url).await;
        match res {
            Err(x) => {
                println!(
                    "block {} failed,retry [{}/10],message {:?}",
                    idx,
                    trys + 1,
                    x
                );
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
    xor_key: u8,
    dst: String,
    file_size: u64,
) -> Result<()> {
    let sz = urls.len();
    let buf_size = 1024 * 1024 * 10;
    let last_size = file_size - (sz as u64 - 1) * buf_size;

    let base_opener = reqwest::Client::new();
    let mut stream = futures::stream::iter(urls.iter().enumerate().map(|(idx, x)| {
        let opener = base_opener.clone();
        async move {
            let text = download_single(opener, idx, x).await?;

            let mut buf = super::decode(&text)?;
            if idx == sz - 1 {
                buf.resize(last_size as usize, 0);
            }
            for i in buf.iter_mut() {
                *i ^= xor_key;
            }
            println!("{} block size = {} bytes", idx, buf.len());

            Result::Ok(buf)
        }
    }))
        .buffered(15);
    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(dst).await?);

    while let Some(x) = stream.next().await {
        let buf = x?;
        writer.write_all(&buf).await?;
    }
    writer.flush().await?;
    Ok(())
}

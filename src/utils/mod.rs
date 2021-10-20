mod bili;
mod fs;
mod image;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub use bili::create_opener;
pub use bili::download_batch;
pub use bili::download_single;
pub use bili::get_user_info;
pub use bili::upload_batch;
pub use bili::upload_single;
pub use fs::generate_idx;
pub use image::decode;
pub use image::encode;

pub use fs::decrypt_aes_single;

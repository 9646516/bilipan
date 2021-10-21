mod bili;
mod image;
mod misc;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub use bili::create_opener;
pub use bili::download_batch;
pub use bili::get_index;
pub use bili::get_user_info;
pub use bili::upload_batch;
pub use bili::upload_single;

pub use image::decode;
pub use image::encode;

pub use misc::combine;
pub use misc::decrypt_aes_single;
pub use misc::generate_idx;
pub use misc::get_input;
pub use misc::split;

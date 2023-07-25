use crate::state::AppState;

use super::error;

use serde::Serialize;
use tokio::io::AsyncReadExt;

use std::io;

use std::collections::HashMap;

pub async fn write_to_cache<T, Y>(records: &HashMap<T, Y>) -> io::Result<()>
where
    T: Serialize,
    Y: Serialize,
{
    let mut records_cache = tokio::fs::File::create(".cache/data").await.unwrap();

    let mut buf: Vec<u8> = Vec::with_capacity(200);
    bincode::serialize_into(&mut buf, records).map_err(|err| error::io_other(&err.to_string()))?;

    let bytes_written = tokio::io::copy(&mut buf.as_slice(), &mut records_cache).await?;

    tracing::debug!("state cache size: {}", bytes_written);

    Ok(())
}

pub async fn fetch_cache() -> AppState {
    let records = if let Ok(file) = tokio::fs::File::open(".cache/data").await.as_mut() {
        let mut buf: Vec<u8> = Vec::with_capacity(200);
        file.read_to_end(&mut buf).await.unwrap();

        bincode::deserialize_from(&mut buf.as_slice()).unwrap()
    } else {
        HashMap::new()
    };

    AppState::new(records)
}

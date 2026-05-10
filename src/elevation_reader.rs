use std::sync::Arc;
use pmtiles::{AsyncPmTilesReader, MmapBackend, TileCoord, PmtError};
use oxigdal_core::buffer::RasterBuffer;
use oxigdal_core::types::RasterDataType;
use futures::{Stream, TryStreamExt};
use async_stream::stream;

#[derive(Debug)]
pub enum ElevationReaderError {
    PmTiles(PmtError),
    WebP(String),
    Oxigdal(String),
}

impl From<PmtError> for ElevationReaderError {
    fn from(err: PmtError) -> Self {
        ElevationReaderError::PmTiles(err)
    }
}

impl std::fmt::Display for ElevationReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElevationReaderError::PmTiles(e) => write!(f, "PmTiles error: {}", e),
            ElevationReaderError::WebP(e) => write!(f, "WebP error: {}", e),
            ElevationReaderError::Oxigdal(e) => write!(f, "Oxigdal error: {}", e),
        }
    }
}

impl std::error::Error for ElevationReaderError {}

pub struct ElevationReader {
    tile_size: usize,
    reader: Arc<AsyncPmTilesReader<MmapBackend>>,
}

impl ElevationReader {
    pub async fn new(file: &str, tile_size: usize) -> Result<Self, ElevationReaderError> {
        let reader = AsyncPmTilesReader::new_with_path(file).await?;
        Ok(Self {
            tile_size,
            reader: Arc::new(reader)
        })
    }

    pub fn iter_tiles(&self) -> impl Stream<Item = Result<TileCoord, ElevationReaderError>> {
        stream! {
            let mut entries = self.reader.clone().entries();
            while let Some(entry) = entries.try_next().await.map_err(ElevationReaderError::from)? {
                for tile in entry.iter_coords() {
                    yield Ok(tile.into());
                }
            }
        }
    }

    pub async fn get(&self, tile: TileCoord) -> Result<RasterBuffer, ElevationReaderError> {
        let bytes = self.reader.get_tile(tile).await?.ok_or_else(|| {
            ElevationReaderError::Oxigdal(format!("Tile not found: {:?}", tile))
        })?;
        let decoder = webp::Decoder::new(&bytes);
        let img = decoder.decode().ok_or_else(|| {
            ElevationReaderError::WebP("Failed to decode WebP image".to_string())
        })?;
        let mut elevation = RasterBuffer::zeros(self.tile_size as u64, self.tile_size as u64, RasterDataType::Float64);
        for i in 0..(self.tile_size * self.tile_size) {
            elevation.set_pixel(
                (i / self.tile_size) as u64,
                (i % self.tile_size) as u64,
                (img[3 * i] as f64) * 256.0 + img[3 * i + 1] as f64 + (img[3 * i + 2] as f64) / 256.0 - 32768 as f64
            ).map_err(|_| ElevationReaderError::Oxigdal("Failed to set pixel".to_string()))?;
        }
        Ok(elevation)
    }
}
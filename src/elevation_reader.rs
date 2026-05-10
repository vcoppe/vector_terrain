use std::sync::Arc;
use pmtiles::{AsyncPmTilesReader, MmapBackend, TileCoord, PmtError};
use oxigdal_core::buffer::RasterBuffer;
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
    padding: usize,
    offset: usize,
    reader: Arc<AsyncPmTilesReader<MmapBackend>>,
}

impl ElevationReader {
    pub async fn new(file: &str, tile_size: usize, padding: usize) -> Result<Self, ElevationReaderError> {
        let reader = AsyncPmTilesReader::new_with_path(file).await?;
        Ok(Self {
            tile_size,
            padding,
            offset: tile_size - padding,
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

    pub async fn fill(&self, elevation: &mut RasterBuffer, tile: TileCoord, dy: i32, dx: i32) -> Result<(), ElevationReaderError> {
        let xstart = if dx >= 0 { 0 } else { self.offset } as isize;
        let ystart = if dy >= 0 { 0 } else { self.offset } as isize;
        let xend = if dx <= 0 { self.tile_size } else { self.padding } as isize;
        let yend = if dy <= 0 { self.tile_size } else { self.padding } as isize;
        let xoff = if dx == -1 { - (self.offset as isize) } else if dx == 0 { self.padding as isize } else { (self.padding + self.tile_size) as isize };
        let yoff = if dy == -1 { - (self.offset as isize) } else if dy == 0 { self.padding as isize } else { (self.padding + self.tile_size) as isize };

        let bytes = self.reader.get_tile(tile).await?.ok_or_else(|| {
            ElevationReaderError::Oxigdal(format!("Tile not found: {:?}", tile))
        });

        if bytes.is_err() {
            if dx == 0 && dy == 0 {
                bytes.map(|_| ())
            } else {
                for x in xstart..xend {
                    for y in ystart..yend {
                        elevation.set_pixel(
                            (x + xoff) as u64,
                            (y + yoff) as u64,
                            0.0
                        ).map_err(|_| ElevationReaderError::Oxigdal("Failed to set pixel".to_string()))?;
                    }
                }

                Ok(())
            }
        } else {
            let bytes = bytes.unwrap();
            let decoder = webp::Decoder::new(&bytes);
            let img = decoder.decode().ok_or_else(|| {
                ElevationReaderError::WebP("Failed to decode WebP image".to_string())
            })?;

            for x in xstart..xend {
                for y in ystart..yend {
                    let i = (x * self.tile_size as isize + y) as usize;
                    elevation.set_pixel(
                        (x + xoff) as u64,
                        (y + yoff) as u64,
                        (img[3 * i] as f64) * 256.0 + img[3 * i + 1] as f64 + (img[3 * i + 2] as f64) / 256.0 - 32768 as f64
                    ).map_err(|_| ElevationReaderError::Oxigdal("Failed to set pixel".to_string()))?;
                }
            }

            Ok(())
        }
    }
}
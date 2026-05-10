use std::sync::Arc;
use pmtiles::{AsyncPmTilesReader, MmapBackend, TileCoord};
use webp::Decoder;
use oxigdal_core::buffer::RasterBuffer;
use oxigdal_core::types::RasterDataType;
use futures::{Stream, TryStreamExt};
use async_stream::stream;

pub struct ElevationReader {
    tile_size: usize,
    reader: Arc<AsyncPmTilesReader<MmapBackend>>,
}

impl ElevationReader {
    pub async fn new(file: &str, tile_size: usize) -> Self {
        Self {
            tile_size,
            reader: Arc::new(AsyncPmTilesReader::new_with_path(file).await.unwrap())
        }
    }

    pub fn iter_tiles(&self) -> impl Stream<Item = TileCoord> {
        stream! {
            let mut entries = self.reader.clone().entries();
            while let Some(entry) = entries.try_next().await.unwrap() {
                for tile_coord in entry.iter_coords() {
                    yield tile_coord.into();
                }
            }
        }
    }

    pub async fn get(&self, coord: TileCoord) -> RasterBuffer {
        let bytes = self.reader.get_tile(coord).await.unwrap().unwrap();
        let decoder = Decoder::new(&bytes);
        let img = decoder.decode().unwrap();
        let mut elevation = RasterBuffer::zeros(self.tile_size as u64, self.tile_size as u64, RasterDataType::Float64);
        for i in 0..(self.tile_size * self.tile_size) {
            elevation.set_pixel((i / self.tile_size) as u64, (i % self.tile_size) as u64, (img[3 * i] as f64) * 256.0 + img[3 * i + 1] as f64 + (img[3 * i + 2] as f64) / 256.0 - 32768 as f64);
        }
        elevation
    }
}
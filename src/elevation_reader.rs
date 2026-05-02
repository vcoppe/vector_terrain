use pmtiles::{AsyncPmTilesReader, MmapBackend, TileCoord};
use webp::Decoder;

pub struct ElevationReader {
    tile_size: usize,
    reader: AsyncPmTilesReader<MmapBackend>,
}

impl ElevationReader {
    pub async fn new(file: &str, tile_size: usize) -> Self {
        Self {
            tile_size,
            reader: AsyncPmTilesReader::new_with_path(file).await.unwrap()
        }
    }

    pub async fn get(self, coord: TileCoord) -> Vec<f32> {
        let bytes = self.reader.get_tile(coord).await.unwrap().unwrap();
        let decoder = Decoder::new(&bytes);
        let img = decoder.decode().unwrap();
        let size = self.tile_size * self.tile_size;
        let mut elevation = vec![0.0; size];
        for i in 0..size {
            elevation[i] = (img[3 * i] as f32) * 256.0 + img[3 * i + 1] as f32 + (img[3 * i + 2] as f32) / 256.0 - 32768 as f32;
        }
        elevation
    }
}
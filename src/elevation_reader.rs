use pmtiles::{AsyncPmTilesReader, MmapBackend, TileCoord};
use webp::Decoder;
use ndarray::Array2;

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

    pub async fn get(self, coord: TileCoord) -> Array2<f64> {
        let bytes = self.reader.get_tile(coord).await.unwrap().unwrap();
        let decoder = Decoder::new(&bytes);
        let img = decoder.decode().unwrap();
        let mut elevation = Array2::<f64>::zeros((self.tile_size, self.tile_size));
        for i in 0..(self.tile_size * self.tile_size) {
            elevation[[i / self.tile_size, i % self.tile_size]] = (img[3 * i] as f64) * 256.0 + img[3 * i + 1] as f64 + (img[3 * i + 2] as f64) / 256.0 - 32768 as f64;
        }
        elevation
    }
}
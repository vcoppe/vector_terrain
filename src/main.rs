mod elevation_reader;

use elevation_reader::ElevationReader;
use pmtiles::TileCoord;


#[tokio::main]
async fn main() {
    let file = "planet.pmtiles";
    let tile_size = 512;
    let reader = ElevationReader::new(file, tile_size).await;
    
    let coord = TileCoord::new(12, 2078, 1554).unwrap();
    let data = reader.get(coord).await;
}
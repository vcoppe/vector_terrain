mod elevation_reader;
mod tile_encoder;

use contour::ContourBuilder;
use futures::{StreamExt, TryStreamExt};
use oxigdal_algorithms::raster::swiss_hillshade;
use oxigdal_core::buffer::RasterBuffer;
use oxigdal_core::types::RasterDataType;
use pmtiles::TileCoord;
use std::sync::Arc;
use tokio::sync::Semaphore;

use elevation_reader::ElevationReader;
use tile_encoder::TileEncoder;

const TILE_SIZE: usize = 512;
const PADDING: usize = 16;
const THRESHOLDS: [f64; 4] = [96.0, 112.0, 140.0, 256.0];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reader = Arc::new(
        ElevationReader::new("planet.pmtiles", TILE_SIZE, PADDING).await?
    );

    let encoder = Arc::new(tokio::sync::Mutex::new(
        TileEncoder::new("example.pmtiles", TILE_SIZE, PADDING)?
    ));

    // limit concurrent CPU jobs
    let semaphore = Arc::new(Semaphore::new(num_cpus::get()));

    let tiles = reader.iter_tiles();

    tiles
        .map_err(|e| Box::<dyn std::error::Error>::from(e))
        .for_each_concurrent(num_cpus::get(), |tile_result| {
            let reader = reader.clone();
            let encoder = encoder.clone();
            let semaphore = semaphore.clone();

            async move {
                let permit = semaphore.acquire().await.unwrap();

                let tile = match tile_result {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("tile error: {e}");
                        return;
                    }
                };

                let mut elevation = RasterBuffer::zeros(
                    (TILE_SIZE + 2 * PADDING) as u64,
                    (TILE_SIZE + 2 * PADDING) as u64,
                    RasterDataType::Float64,
                );

                // async reads
                for dx in -1i32..=1 {
                    for dy in -1i32..=1 {
                        if tile.x() as i32 + dx < 0 || tile.y() as i32 + dy < 0 {
                            continue;
                        }

                        let shifted_tile = match TileCoord::new(
                            tile.z(),
                            (tile.x() as i32 + dx) as u32,
                            (tile.y() as i32 + dy) as u32,
                        ) {
                            Ok(t) => t,
                            Err(_) => continue,
                        };

                        if let Err(e) = reader.fill(
                            &mut elevation,
                            shifted_tile,
                            dx,
                            dy,
                        ).await {
                            eprintln!("fill error: {e}");
                            return;
                        }
                    }
                }

                // move CPU-heavy work off async executor
                let bands = tokio::task::spawn_blocking(move || {
                    let mut hillshade =
                        swiss_hillshade(&elevation, 1.0, 30.0)
                            .unwrap();

                    for i in 0..hillshade.height() {
                        for j in 0..hillshade.width() {
                            let val = hillshade.get_pixel(i, j).unwrap();
                            hillshade
                                .set_pixel(i, j, 255.0 - val)
                                .unwrap();
                        }
                    }

                    let c = ContourBuilder::new(
                        TILE_SIZE + 2 * PADDING,
                        TILE_SIZE + 2 * PADDING,
                        true,
                    );

                    c.isobands(hillshade.as_slice().unwrap(), &THRESHOLDS)
                        .unwrap()
                })
                .await
                .unwrap();

                // serialize writes if encoder isn't thread-safe
                {
                    let mut enc = encoder.lock().await;

                    if let Err(e) = enc.encode(tile, &bands) {
                        eprintln!("encode error: {e}");
                    }
                }

                drop(permit);
            }
        })
        .await;

    Arc::try_unwrap(encoder)
        .unwrap()
        .into_inner()
        .finalize()?;

    Ok(())
}
use mvt::{Error, GeomEncoder, GeomType, Tile};
use pmtiles::{PmTilesStreamWriter, PmTilesWriter, TileCoord, TileType};
use std::fs::File;
use contour::Band;
use geo_types::LineString;

const EXPANSION_FACTOR: usize = 8; // to avoid snapping coordinates to the 512x512 grid
const EXPANSION_FACTOR_FLOAT: f64 = EXPANSION_FACTOR as f64;

pub struct TileEncoder {
    writer: PmTilesStreamWriter<File>,
}

impl TileEncoder {

    pub fn new(path: &str) -> Self {
        let file = File::create(path).unwrap();
        let writer = PmTilesWriter::new(TileType::Mvt).create(file).unwrap();
        Self {
            writer,
        }
    }

    pub fn encode(&mut self, tile_size: usize, tile_coord: TileCoord, bands: &Vec<Band>) -> Result<(), Error> {
        let mut tile = Tile::new((tile_size * EXPANSION_FACTOR) as u32);
        let mut layer = tile.create_layer("hillshading");

        for band in bands.iter() {
            for polygon in band.geometry().iter() {
                let mut b = GeomEncoder::new(GeomType::Polygon);
                b = Self::add_linestring(b, polygon.exterior())?;
                for interior in polygon.interiors() {
                    b = Self::add_linestring(b, interior)?;
                }
                let data = b.encode()?;
                let feature = layer.into_feature(data);
                layer = feature.into_layer();
            }
        }
        tile.add_layer(layer)?;
        let data = tile.to_bytes()?;

        self.writer.add_tile(tile_coord, &data).inspect_err(|e| eprintln!("failed to write tile {tile_coord:?}: {e}"));

        Ok(())
    }

    pub fn finalize(self) -> Result<(), Error> {
        self.writer.finalize().inspect_err(|e| eprintln!("couldn't finalize pmtiles: {e}"));

        Ok(())
    }

    fn add_linestring(mut encoder: GeomEncoder<f64>, line: &LineString<f64>) -> Result<GeomEncoder<f64>, Error> {
        for coord in line.coords() {
            encoder = encoder.point(coord.y * EXPANSION_FACTOR_FLOAT, coord.x * EXPANSION_FACTOR_FLOAT)?;
        }
        encoder.complete_geom()?;
        Ok(encoder)
    }
}
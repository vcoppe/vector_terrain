use mvt::{Error as MvtError, GeomEncoder, GeomType, Tile};
use pmtiles::{PmTilesStreamWriter, PmTilesWriter, TileCoord, TileType, PmtError};
use std::fs::File;
use contour::Band;
use geo_types::LineString;

const EXPANSION_FACTOR: usize = 8; // to avoid snapping coordinates to the 512x512 grid
const EXPANSION_FACTOR_FLOAT: f64 = EXPANSION_FACTOR as f64;

#[derive(Debug)]
pub enum TileEncoderError {
    Mvt(MvtError),
    PmTiles(PmtError),
    Io(std::io::Error),
}

impl From<MvtError> for TileEncoderError {
    fn from(err: MvtError) -> Self {
        TileEncoderError::Mvt(err)
    }
}

impl From<PmtError> for TileEncoderError {
    fn from(err: PmtError) -> Self {
        TileEncoderError::PmTiles(err)
    }
}

impl From<std::io::Error> for TileEncoderError {
    fn from(err: std::io::Error) -> Self {
        TileEncoderError::Io(err)
    }
}

impl std::fmt::Display for TileEncoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TileEncoderError::Mvt(e) => write!(f, "MVT error: {}", e),
            TileEncoderError::PmTiles(e) => write!(f, "PmTiles error: {}", e),
            TileEncoderError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for TileEncoderError {}

pub struct TileEncoder {
    writer: PmTilesStreamWriter<File>,
}

impl TileEncoder {

    pub fn new(path: &str) -> Result<Self, TileEncoderError> {
        let file = File::create(path)?;
        let writer = PmTilesWriter::new(TileType::Mvt).create(file)?;
        Ok(Self {
            writer,
        })
    }

    pub fn encode(&mut self, tile_size: usize, tile_coord: TileCoord, bands: &Vec<Band>) -> Result<(), TileEncoderError> {
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

        self.writer.add_tile(tile_coord, &data)?;

        Ok(())
    }

    pub fn finalize(self) -> Result<(), TileEncoderError> {
        self.writer.finalize()?;

        Ok(())
    }

    fn add_linestring(mut encoder: GeomEncoder<f64>, line: &LineString<f64>) -> Result<GeomEncoder<f64>, TileEncoderError> {
        for coord in line.coords() {
            encoder = encoder.point(coord.y * EXPANSION_FACTOR_FLOAT, coord.x * EXPANSION_FACTOR_FLOAT)?;
        }
        encoder.complete_geom()?;
        Ok(encoder)
    }
}
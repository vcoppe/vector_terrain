use mvt::{Error, GeomEncoder, GeomType, Tile};
use pmtiles::{PmTilesWriter, TileType, TileCoord};
use std::fs::File;
use contour::Band;

pub struct TileEncoder {}

impl TileEncoder {

    pub fn encode(tile_size: usize, tile_coord: TileCoord, bands: &Vec<Band>, names: &[&str; 3]) -> Result<(), Error> {
        let mut tile = Tile::new(tile_size as u32);
        let mut layer = tile.create_layer("hillshading");

        for band in bands.iter() {
            let intensity_val = band.max_v().to_string();
            println!("{} {} {}", band.min_v(), band.max_v(), intensity_val);
            for polygon in band.geometry().iter() {
                let mut b = GeomEncoder::new(GeomType::Polygon);
                for coord in polygon.exterior().coords() {
                    b = b.point(coord.y, coord.x)?;
                }
                b.complete_geom()?;
                for interior in polygon.interiors() {
                    for coord in interior.coords() {
                        b = b.point(coord.y, coord.x)?;
                    }
                    b.complete_geom()?;
                }
                let data = b.encode()?;
                let mut feature = layer.into_feature(data);
                feature.add_tag_string("intensity", intensity_val.as_str());
                layer = feature.into_layer();
            }
        }
        tile.add_layer(layer)?;
        let data = tile.to_bytes()?;

        let file = File::create("example.pmtiles").unwrap();
        let mut writer = PmTilesWriter::new(TileType::Mvt).create(file).unwrap();
        writer.add_tile(tile_coord, &data).unwrap();
        writer.finalize().unwrap();

        Ok(())
    }
}
#!/bin/bash

cargo build --release

./target/release/vector_terrain --input planet.pmtiles --hillshading --contours-m --contours-ft
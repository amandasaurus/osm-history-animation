# osm-history-animation

Create gifs of OpenStreetMap editing activity

    cargo run --release --  -i history-latest.osm.pbf -o planet.gif -h 375 -s $(( 7 * 24 * 3600 )) --colour-ramp ./colours

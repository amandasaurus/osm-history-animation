extern crate osmio;
extern crate image;
extern crate gif;
extern crate clap;

use clap::{Arg, App};

use std::fs;
use osmio::OSMReader;
use osmio::pbf::PBFReader;
use std::io::BufReader;
use std::collections::{HashMap, HashSet};

use gif::SetParameter;

type Frames = Vec<(u32, Vec<u32>)>;


fn read_file(filename: &str, height: u32, sec_per_frame: u32, bbox: &[f32; 4]) -> Frames {
    let file = BufReader::new(fs::File::open(&filename).unwrap());
    let mut node_reader = PBFReader::new(file);
    let node_reader = node_reader.nodes();
    
    println!("Parsing {}", filename);

    let mut results: HashMap<u32, HashSet<u32>> = HashMap::new();

    let left = bbox[0]; let bottom = bbox[1]; let right = bbox[2]; let top = bbox[3];
    let bbox_width = right - left;
    let bbox_height = top - bottom;
    let width = ((bbox_width / bbox_height) * (height as f32)) as u32;

    // 1st April 2005, midnight GMT. We presume no OSM editing before then.
    let osm_epoch = 1109635200;

    // FIXME use a btreemap?
    
    let mut first_frame_no = std::u32::MAX;
    let mut last_frame_no = 0;

    let mut num_nodes = 0;
    for node in node_reader {
        if let (Some(lat), Some(lon)) = (node.lat, node.lon) {
            if lat > top || lat < bottom || lon > right || lon < left {
                continue;
            }

            let timestamp = node.timestamp.to_epoch_number() as u64;
            if timestamp < osm_epoch {
                panic!("timestamp before epoch. Change code. {}", timestamp);
            }
            let offset = (timestamp - osm_epoch) as u32;
            // TODO double check that this rounds down
            let frame_no = offset / sec_per_frame;

            if frame_no < first_frame_no {
                first_frame_no = frame_no;
            }
            if frame_no > last_frame_no {
                last_frame_no = frame_no;
            }

            let pixel_idx = latlon_to_pixel_index(lat, lon, width, height, &bbox);
            results.entry(frame_no).or_insert(HashSet::new()).insert(pixel_idx);

            num_nodes += 1;
            if num_nodes % 10_000_000 == 0 {
                println!("Done {} nodes", num_nodes);
            }
        }
    }
    let num_frames = last_frame_no - first_frame_no + 1;
    println!("There are {} frames, which is {} sec", num_frames, num_frames as f32/30.);

    let mut sorted_results = Vec::with_capacity((last_frame_no-first_frame_no+1) as usize);

    for frame_no in 0..num_frames {
        match results.remove(&(frame_no+first_frame_no)) {
            None => { sorted_results.push((frame_no, Vec::with_capacity(0))) },
            Some(pixels) => {
                let pixels = pixels.into_iter().collect();
                sorted_results.push((frame_no, pixels));
            }
        }
    }

    sorted_results
}

#[inline(always)]
fn age_to_colour(age: &Option<u32>) -> [u8; 4] {
    match *age {
        None => [0, 0, 0, 0xff],
        Some(age) => {
            let age = age * 3;
            if age > 255 {
                [0, 0, 0, 0xff]
            } else {
                [(255 - age) as u8, 0, 0, 0xff]
            }
        }
    }
}

fn latlon_to_pixel_index(lat: f32, lon: f32, width: u32, height: u32, bbox: &[f32; 4]) -> u32 {
    let left = bbox[0]; let bottom = bbox[1]; let right = bbox[2]; let top = bbox[3];
    assert!(top > bottom);
    let bbox_width = right - left;
    let bbox_height = top - bottom;
    let lat = top - lat;
    let lon = lon - left;


    let x = ((lon/bbox_width)*(width as f32)) as u32;
    let y = ((lat/bbox_height)*(height as f32)) as u32;

    //println!("lat = {} lon = {} x = {} y = {} y*width+x = {}", lat, lon, x, y, y*width+x);
    let i = y * width + x;

    i
}

fn create_equirectangular_map(frames: Frames, output_image_filename: &str, height: u32, bbox: &[f32; 4]) {
    let mut output_file = fs::File::create(output_image_filename).expect("Can't create image");

    let left = bbox[0]; let bottom = bbox[1]; let right = bbox[2]; let top = bbox[3];
    let bbox_width = right - left;
    let bbox_height = top - bottom;
    let width = ((bbox_width / bbox_height) * (height as f32)) as u32;

    // FIXME change width/height to u16?
    let mut encoder = gif::Encoder::new(&mut output_file, width as u16, height as u16, &[]).expect("Couldn't create encoder");
    encoder.set(gif::Repeat::Infinite).expect("Couldn't get inf repeat");

    let mut image = vec![None; (width*height) as usize];

    for (frame_no, pixels) in frames.into_iter() {
        for i in pixels {
            image[i as usize] = Some(frame_no);
        }

        let mut pixels = Vec::with_capacity(image.len() * 4);
        for p in image.iter().cloned() {
            pixels.extend(age_to_colour(&p.clone()).iter());
        }

        let mut frame = gif::Frame::from_rgba(width as u16, height as u16, pixels.as_mut_slice());
        // 30 fps, and delay is in units of 10ms.
        frame.delay = 100 / 30;

        encoder.write_frame(&frame).expect("Couldn't write frame");

        println!("Wrote frame {}", frame_no);

    }

}

fn main() {
    let matches = App::new("osm-history-animation")
                          .arg(Arg::with_name("pbf_file").long("pbf-file").short("i")
                               .takes_value(true))
                          .arg(Arg::with_name("output_image") .long("output-image").short("o")
                               .takes_value(true))
                          .arg(Arg::with_name("height") .long("height").short("h")
                               .takes_value(true))
                          .arg(Arg::with_name("spf") .long("sec-per-frame").short("s")
                               .takes_value(true))
                          .get_matches();

    let input_filename = matches.value_of("pbf_file").unwrap();
    let output_image = matches.value_of("output_image").unwrap();
    let height: u32 = matches.value_of("height").unwrap().parse().unwrap();
    let frames_per_sec: u32 = matches.value_of("spf").unwrap().parse().unwrap();

    let bbox = [-13.1, 49.28, -3.26, 56.69];

    let frames = read_file(&input_filename, height, frames_per_sec, &bbox);

    println!("Creating image {}", output_image);

    create_equirectangular_map(frames, &output_image, height, &bbox);

    println!("\nFinished");
}

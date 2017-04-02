extern crate osmio;
extern crate image;
extern crate gif;

use std::fs;
use osmio::OSMReader;
use osmio::pbf::PBFReader;
use std::env::args;
use std::io::BufReader;
use std::collections::{HashMap, HashSet};

use gif::SetParameter;

type Frames = Vec<(u32, Vec<u32>)>;


fn read_file(filename: &str, height: u32, sec_per_frame: u32) -> Frames {
    let file = BufReader::new(fs::File::open(&filename).unwrap());
    let mut node_reader = PBFReader::new(file);
    let node_reader = node_reader.nodes();
    
    println!("Parsing {}", filename);

    let mut results: HashMap<u32, HashSet<u32>> = HashMap::new();

    // 1st April 2005, midnight GMT. We presume no OSM editing before then.
    let osm_epoch = 1109635200;

    let width = height * 2;

    // FIXME use a btreemap?
    
    let mut first_frame_no = std::u32::MAX;
    let mut last_frame_no = 0;

    let mut num_nodes = 0;
    for node in node_reader {
        if let (Some(lat), Some(lon)) = (node.lat, node.lon) {
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

            let pixel_idx = latlon_to_pixel_index(lat, lon, width, height);
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
fn age_to_colour(curr_value: u32, frame_no: u32) -> [u8; 3] {
    if curr_value == 0 {
        // This means the pixel has never been used
        [0, 0, 0]
    } else {
        let age = frame_no - curr_value;
        if age > 255 {
            [0, 0, 0]
        } else {
            [(255 - age) as u8, 0, 0]
        }

    }
}

fn latlon_to_pixel_index(lat: f32, lon: f32, width: u32, height: u32) -> u32 {
    // update the image
    let x = (((lon + 90.)/180.)*(width as f32)) as u32;
    let y = (((lat + 180.)/360.)*(height as f32)) as u32;

    //println!("lat = {} lon = {} x = {} y = {} y*width+x = {}", lat, lon, x, y, y*width+x);
    let i = y * width + x;

    i
}

fn create_equirectangular_map(frames: Frames, output_image_filename: &str, height: u32) {
    let mut output_file = fs::File::create(output_image_filename).expect("Can't create image");

    let width = height * 2;
    // FIXME change width/height to u16?
    let mut encoder = gif::Encoder::new(&mut output_file, width as u16, height as u16, &[]).expect("Couldn't create encoder");
    encoder.set(gif::Repeat::Infinite).expect("Couldn't get inf repeat");

    // 0 means never done.
    let mut image = vec![0u32; (width*height) as usize];

    for (frame_no, pixels) in frames.into_iter() {
        for i in pixels {
            image[i as usize] = frame_no;
        }

        // Write out a new frame
        let img = image::ImageBuffer::from_fn(width, height, |x, y| {
            //println!("x = {} y = {} width = {} height = {} x*width+y = {}", x, y, width, height, x*width+y);
            let curr_value = image[(y*width+x) as usize];
            image::Rgb(age_to_colour(curr_value, frame_no))
        });

        //let mut f = fs::File::create(format!("{}-frame{:08}.png", output_image_filename, frame_no)).expect("Couldn't create frame");
        //image::ImageRgb8(img.clone()).save(&mut f, image::PNG).expect("Couldn't save image");
        //frame_no += 1;

        let mut frame = gif::Frame::from_rgb(width as u16, height as u16, &img.into_vec());
        // 30 fps, and delay is in units of 10ms.
        frame.delay = 100 / 30;

        encoder.write_frame(&frame).expect("Couldn't write frame");

        println!("Wrote frame {}", frame_no);
        //println!("Wrote frame for timestamp {} ({})", iso, timestamp);

    }

}

fn main() {
    let input_filename = args().nth(1).expect("Missing inputfilename");
    let output_image = args().nth(2).expect("Missing output image");
    let height: u32 = args().nth(3).expect("need to provide a height").parse().expect("Not an integer");
    let frames_per_sec: u32 = args().nth(4).expect("need to provide a speed").parse().expect("Not an integer");

    let frames = read_file(&input_filename, height, frames_per_sec);

    println!("Creating image {}", output_image);

    create_equirectangular_map(frames, &output_image, height);

    println!("\nFinished");
}

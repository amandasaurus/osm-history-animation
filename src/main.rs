extern crate osmio;
extern crate image;
extern crate gif;
extern crate clap;

use clap::{Arg, App};

use std::fs;
use osmio::OSMReader;
use osmio::pbf::PBFReader;
use std::io::{Read, Write, BufReader, BufRead, BufWriter};
use std::collections::{HashMap, HashSet};

use gif::SetParameter;

type Frames = Vec<(u32, Vec<u32>)>;

struct ColourRamp {
    empty_colour: (u8, u8, u8),
    steps: Vec<(u32, (u8, u8, u8))>,
}

impl ColourRamp {
    fn new_from_filename(filename: &str) -> Self {
        let mut contents = String::new();
        let mut file = fs::File::open(filename).unwrap();
        file.read_to_string(&mut contents).expect("Couldn't read colour ramp file");
        Self::new_from_text(&contents)

    }
    fn new_from_text(source: &str) -> Self {
        let lines: Vec<_> = source.lines().collect();
        let empty_vec = lines[0].split(",").filter_map(|x| x.parse::<u8>().ok()).take(3).collect::<Vec<_>>();
        let empty = (empty_vec[0], empty_vec[1], empty_vec[2]);

        let mut steps = Vec::new();
        for line in lines.iter().skip(1) {
            let line = line.split(",").filter_map(|x| x.parse::<u32>().ok()).take(4).collect::<Vec<_>>();
            let age = line[0];
            let colour = (line[1] as u8, line[2] as u8, line[3] as u8);
            steps.push((age, colour));
        }

        if steps.len() > 254 {
            panic!("Too many steps");
        }

        ColourRamp{ empty_colour: empty, steps: steps }
    }

    fn palette(&self) -> Vec<u8> {
        let mut results = Vec::with_capacity((self.steps.len()+1)*3);
        results.push(self.empty_colour.0);
        results.push(self.empty_colour.1);
        results.push(self.empty_colour.2);

        for &(_, (r, g, b)) in self.steps.iter() {
            results.push(r);
            results.push(g);
            results.push(b);
        }

        results
    }

    fn index_for_age(&self, age: Option<u32>) -> u8 {
        match age {
            None => 0,
            Some(age) => match self.steps.iter().position(|&(x, _)| x == age) {
                None => 0,
                Some(i) => i as u8,
            }
        }
    }
}

fn read_pbf(filename: &str, height: u32, sec_per_frame: u32, bbox: &[f32; 4]) -> Frames {
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

fn write_frames(frames: Frames, filename: &str) {
    let mut file = BufWriter::new(fs::File::create(&filename).unwrap());
    for (frame_no, pixels) in frames.into_iter() {
        write!(file, "{},", frame_no).unwrap();
        for p in pixels {
            write!(file, "{},", p).unwrap();
        }
        write!(file, "\n").unwrap()
    }
}

fn read_frames(filename: &str) -> Frames {
    let file = BufReader::new(fs::File::open(&filename).unwrap());
    let mut results = Frames::new();

    for line in file.lines() {
        let line = line.unwrap();
        let mut nums: Vec<u32> = line.split(",").filter_map(|s| s.parse().ok()).collect();
        let frame_no = nums.remove(0);
        let pixels = nums;
        results.push((frame_no, pixels))
    }

    results
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

fn create_equirectangular_map(frames: Frames, output_image_filename: &str, height: u32, bbox: &[f32; 4], colour_ramp: &ColourRamp) {
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
            let index = colour_ramp.index_for_age(p.map(|t| frame_no - t));
            pixels.push(index);
        }

        let mut frame = gif::Frame::from_palette_pixels(width as u16, height as u16, &colour_ramp.palette(), pixels.as_mut_slice());
        // 30 fps, and delay is in units of 10ms.
        frame.delay = 100 / 30;

        encoder.write_frame(&frame).expect("Couldn't write frame");

        if frame_no % 30 == 0 {
            println!("Wrote frame {}", frame_no);
        }

    }

}

fn main() {
    let matches = App::new("osm-history-animation")
                          .arg(Arg::with_name("input").long("input").short("i")
                               .takes_value(true))
                          .arg(Arg::with_name("output") .long("output").short("o")
                               .takes_value(true))
                          .arg(Arg::with_name("height") .long("height").short("h")
                               .takes_value(true))
                          .arg(Arg::with_name("spf") .long("sec-per-frame").short("s")
                               .takes_value(true))
                          .arg(Arg::with_name("colour_ramp") .long("colour-ramp")
                               .takes_value(true))
                          .arg(Arg::with_name("save-intermediate").long("save-intermediate"))
                          .arg(Arg::with_name("load-intermediate").long("load-intermediate"))
                          .arg(Arg::with_name("bbox").long("bbox"))
                          .get_matches();

    let input_filename = matches.value_of("input").unwrap();
    let output_filename = matches.value_of("output").unwrap();
    let height: u32 = matches.value_of("height").unwrap().parse().unwrap();
    let frames_per_sec: u32 = matches.value_of("spf").unwrap().parse().unwrap();

    let bbox = match matches.value_of("bbox") {
        None => [-180., -90., 180., 90.],
        Some(text) => {
            let coords: Vec<f32> = text.split(",").map(|x| x.parse().unwrap() ).collect();
            [coords[0], coords[1], coords[2], coords[3]]
        }
    };

    //let bbox = [-13.1, 49.28, -3.26, 56.69];


    let frames = if matches.is_present("load-intermediate") {
        println!("Reading frames from {}", input_filename);
        read_frames(&input_filename)
    } else {
        println!("Reading frames from {}", input_filename);
        read_pbf(&input_filename, height, frames_per_sec, &bbox)
    };

    if matches.is_present("save-intermediate") {
        println!("Saving frame details to {}", output_filename);
        write_frames(frames, &output_filename);
    } else {
        let colour_ramp = ColourRamp::new_from_filename(matches.value_of("colour_ramp").unwrap());
        println!("Creating image {}", output_filename);
        create_equirectangular_map(frames, &output_filename, height, &bbox, &colour_ramp);
    }
    println!("\nFinished");
}

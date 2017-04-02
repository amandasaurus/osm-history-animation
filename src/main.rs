extern crate osmio;
extern crate image;
extern crate gif;

use std::fs;
use osmio::OSMReader;
use osmio::pbf::PBFReader;
use std::env::args;
use std::io::{Write, BufReader, BufWriter, BufRead};

use gif::SetParameter;


fn read_file(filename: &str, output_csv: &str) {
    let file = BufReader::new(fs::File::open(&filename).unwrap());
    let mut node_reader = PBFReader::new(file);
    let node_reader = node_reader.nodes();

    let mut output_file = BufWriter::new(fs::File::create(&output_csv).expect("Could not open output CSV"));

    let mut num_nodes = 0;
    for node in node_reader {
        // Might be quicker to use binary search thing
        if let (Some(lat), Some(lon)) = (node.lat, node.lon) {
            let timestamp = node.timestamp.to_epoch_number();
            writeln!(output_file, "{},{},{}", timestamp, lat, lon).expect("Could not write line");
            num_nodes += 1;
            if num_nodes % 10_000_000 == 0 {
                println!("Written {} nodes", num_nodes);
            }
        }
    }
}

#[allow(unused_variables)]
fn age_to_colour(age: u32) -> [u8; 3] {
    if age < 7*24*60*60 {
        [255, 0, 0]
    } else {
        [0, 0, 0]
    }
}

fn create_equirectangular_map(input_csv_filename: &str, output_image_filename: &str, height: u32, frames_per_sec: u32) {
    let input_csv = BufReader::new(fs::File::open(input_csv_filename).expect("Cannot open CSV file"));
    let mut output_file = fs::File::create(output_image_filename).expect("Can't create image");
    let width = height * 2;
    // FIXME change width/height to u16?
    let mut encoder = gif::Encoder::new(&mut output_file, width as u16, height as u16, &[]).expect("Couldn't create encoder");
    encoder.set(gif::Repeat::Infinite).expect("Couldn't get inf repeat");

    //let mut frame_no = 0;
    let mut image = vec![0u32; (width*height) as usize];
    let mut timestamp_of_last_frame = 0;

    for line in input_csv.lines() {
        let line = line.expect("Cannot read line");
        let nums: Vec<_> = line.split(",").take(3).collect();
        let timestamp: u32 = nums[0].parse().expect("not a number");
        let lat: f32 = nums[1].parse().expect("not a number");
        let lon: f32 = nums[2].parse().expect("not a number");
        //println!("Doing change for timestamp {}", timestamp);
        
        if lat > 90. || lat < -90. || lon > 180. || lon < -180. {
            //println!("Bad location lat = {} lon = {}", lat, lon);
            // WTF bad data?
            continue;
        }

        if timestamp_of_last_frame == 0 {
            timestamp_of_last_frame = timestamp;
        }

        if timestamp < timestamp_of_last_frame {
            panic!("Input CSV file isn't sorted! run 'sort -o {} {}' to sort it", input_csv_filename, input_csv_filename);
        }

        if timestamp - timestamp_of_last_frame >= frames_per_sec {
            // Write out a new frame
            let img = image::ImageBuffer::from_fn(width, height as u32, |x, y| {
                //println!("x = {} y = {} width = {} height = {} x*width+y = {}", x, y, width, height, x*width+y);
                let curr_value = image[(y*width+x) as usize];
                let age = timestamp - curr_value;
                image::Rgb(age_to_colour(age))
            });

            //let mut f = fs::File::create(format!("{}-frame{:08}.png", output_image_filename, frame_no)).expect("Couldn't create frame");
            //image::ImageRgb8(img.clone()).save(&mut f, image::PNG).expect("Couldn't save image");
            //frame_no += 1;

            let mut frame = gif::Frame::from_rgb(width as u16, height as u16, &img.into_vec());
            // 30 fps, and delay is in units of 10ms.
            frame.delay = 100 / 30;

            encoder.write_frame(&frame).expect("Couldn't write frame");


            timestamp_of_last_frame = timestamp;
            let iso = osmio::utils::epoch_to_iso(timestamp_of_last_frame as i32);
            println!("Wrote frame for timestamp {} ({})", iso, timestamp);
        }


        //let lat = lat * -1.;

        // update the image
        let x = (((lon + 90.)/180.)*(width as f32)) as u32;
        let y = (((lat + 180.)/360.)*(height as f32)) as u32;

        //println!("lat = {} lon = {} x = {} y = {} y*width+x = {}", lat, lon, x, y, y*width+x);
        let i = (y * width + x) as usize;
        image[i] = timestamp;

    }

}

fn main() {
    let command = args().nth(1).expect("Need a command");
    if command == "parsepbf" {
        let input_filename = args().nth(2).expect("Missing inputfilename");
        let output_csv = args().nth(3).expect("Missing csv output");
        println!("Writing node history CSV to {}", output_csv);
        read_file(&input_filename, &output_csv);
        // FIXME call sort(1) here
    } else if command == "createimage" {
        let input_csv = args().nth(2).expect("Missing input csv");
        let output_image = args().nth(3).expect("Missing output image");
        let height: u32 = args().nth(4).expect("need to provide a height").parse().expect("Not an integer");
        let frames_per_sec: u32 = args().nth(5).expect("need to provide a speed").parse().expect("Not an integer");
        //let  = args().nth(3).expect("Missing output image");
        println!("Creating image from {}", input_csv);
        create_equirectangular_map(&input_csv, &output_image, height, frames_per_sec);
    } else {
        panic!("Nothing to do");

    }

    println!("\nFinished");
}

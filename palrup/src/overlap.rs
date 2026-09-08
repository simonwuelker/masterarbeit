use std::path::Path;

use anyhow::Context;
use apache_datasketches::theta::{ThetaIntersection, ThetaSketch, ThetaSketchBuilder};

use crate::palrup::{PalrupIterator, Step};

pub(crate) fn overlap<P1, P2>(first: P1, second: P2) -> anyhow::Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    println!(
        "Compute overlap between {} and {}",
        first.as_ref().display(),
        second.as_ref().display()
    );

    let first_sketch = sketch_from_file(first)?;
    let second_sketch = sketch_from_file(second)?;

    let mut intersection = ThetaIntersection::new();
    intersection.update(&first_sketch);
    intersection.update(&second_sketch);
    let result = intersection.get_result(false).unwrap();
    println!("Estimated overlap: {:?}", result.get_estimate());

    Ok(())
}

fn sketch_from_file<P>(file: P) -> anyhow::Result<ThetaSketch>
where
    P: AsRef<Path>,
{
    let mut sketch = ThetaSketchBuilder::new().lg_k(12).build()?;

    let iterator = PalrupIterator::for_file(file)?;

    for step in iterator {
        let step = step?;

        let Step::Add(add) = step else {
            continue;
        };
        sketch.update_bytes(bytemuck::cast_slice(&add.hints));
    }

    Ok(sketch)
}

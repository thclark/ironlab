use ironlab_scene::maths::colormap::{
    CIVIDIS, COOLWARM, GRAY, INFERNO, Lut, MAGMA, PLASMA, VIRIDIS, normalise, sample,
};

use crate::assert_close;

/// CIE L* (0 to 100) of an sRGB colour.
fn lightness(rgb: [u8; 3]) -> f64 {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let y = 0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2]);
    if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    }
}

// Why: the tables are generated data; pinning the published endpoint colours of each map proves
// the right map was generated with the right quantisation and orientation (low to high).
#[test]
fn tables_match_published_endpoints() {
    assert_eq!(VIRIDIS[0], [68, 1, 84]);
    assert_eq!(VIRIDIS[255], [253, 231, 37]);
    assert_eq!(CIVIDIS[0], [0, 34, 78]);
    assert_eq!(CIVIDIS[255], [254, 232, 56]);
    assert_eq!(MAGMA[0], [0, 0, 4]);
    assert_eq!(MAGMA[255], [252, 253, 191]);
    assert_eq!(INFERNO[0], [0, 0, 4]);
    assert_eq!(INFERNO[255], [252, 255, 164]);
    assert_eq!(PLASMA[0], [13, 8, 135]);
    assert_eq!(PLASMA[255], [240, 249, 33]);
    assert_eq!(COOLWARM[0], [59, 76, 192]);
    assert_eq!(COOLWARM[255], [180, 4, 38]);
    assert_eq!(GRAY[0], [0, 0, 0]);
    assert_eq!(GRAY[255], [255, 255, 255]);
}

// Why: the perceptually uniform sequential maps exist so that lightness increases with value;
// a transcription or ordering error would break that guarantee and mislead readers. Eight-bit
// quantisation allows dips of a few hundredths of L* between neighbours, never a real reversal.
#[test]
fn sequential_maps_increase_monotonically_in_lightness() {
    let maps: [(&str, &Lut); 6] = [
        ("viridis", &VIRIDIS),
        ("cividis", &CIVIDIS),
        ("magma", &MAGMA),
        ("inferno", &INFERNO),
        ("plasma", &PLASMA),
        ("gray", &GRAY),
    ];
    for (name, lut) in maps {
        let l: Vec<f64> = lut.iter().map(|c| lightness(*c)).collect();
        for (k, w) in l.windows(2).enumerate() {
            assert!(
                w[1] - w[0] > -0.1,
                "{name}: lightness falls from entry {k} to {}",
                k + 1
            );
        }
        for (k, w) in l.windows(9).enumerate() {
            assert!(
                w[8] - w[0] > 1.0,
                "{name}: lightness does not rise over entries {k}..{}",
                k + 8
            );
        }
    }
}

// Why: a diverging map must be lightest at the centre and equally dark at both ends, with cool
// hues below and warm hues above, so that equal deviations either side read as equal.
#[test]
fn coolwarm_diverges_from_a_light_centre() {
    let l: Vec<f64> = COOLWARM.iter().map(|c| lightness(*c)).collect();
    let brightest = (0..256).max_by(|&a, &b| l[a].total_cmp(&l[b])).unwrap();
    assert!((120..=136).contains(&brightest), "peak at {brightest}");
    assert_close(l[0], l[255], 1.0);
    assert!(COOLWARM[0][2] > COOLWARM[0][0], "low end is blue");
    assert!(COOLWARM[255][0] > COOLWARM[255][2], "high end is red");
}

// Why: NaN marks missing data, which must be left unpainted rather than coloured as a value.
#[test]
fn sample_of_nan_is_none() {
    assert_eq!(sample(&VIRIDIS, f64::NAN), None);
}

// Why: values outside the colour limits take the end colours (MATLAB clim behaviour), including
// infinities.
#[test]
fn sample_clamps_out_of_range_values() {
    assert_eq!(sample(&VIRIDIS, -3.0), Some(VIRIDIS[0]));
    assert_eq!(sample(&VIRIDIS, f64::NEG_INFINITY), Some(VIRIDIS[0]));
    assert_eq!(sample(&VIRIDIS, 7.0), Some(VIRIDIS[255]));
    assert_eq!(sample(&VIRIDIS, f64::INFINITY), Some(VIRIDIS[255]));
}

// Why: every entry must receive an equal share of [0, 1]; the endpoints and the boundary between
// the two middle entries pin the documented floor(t·256) indexing.
#[test]
fn sample_indexes_equal_bins() {
    assert_eq!(sample(&GRAY, 0.0), Some(GRAY[0]));
    assert_eq!(sample(&GRAY, 1.0), Some(GRAY[255]));
    assert_eq!(sample(&GRAY, 0.5), Some(GRAY[128]));
    assert_eq!(sample(&GRAY, 0.499), Some(GRAY[127]));
    assert_eq!(sample(&GRAY, 1.0 / 256.0), Some(GRAY[1]));
}

// Why: normalisation maps colour limits onto [0, 1] and must keep NaN as NaN so that sample()
// can recognise missing values downstream.
#[test]
fn normalise_maps_limits_to_unit_interval() {
    assert_eq!(normalise(5.0, 0.0, 10.0), 0.5);
    assert_eq!(normalise(0.0, 0.0, 10.0), 0.0);
    assert_eq!(normalise(10.0, 0.0, 10.0), 1.0);
    assert_eq!(normalise(20.0, 0.0, 10.0), 2.0, "normalise does not clamp");
    assert!(normalise(f64::NAN, 0.0, 10.0).is_nan());
}

// Why: a constant field has equal colour limits; it must take one well-defined colour instead of
// producing NaN (which would render as missing data).
#[test]
fn normalise_degenerate_limits_gives_middle() {
    assert_eq!(normalise(3.0, 3.0, 3.0), 0.5);
    assert_eq!(normalise(-7.0, 3.0, 3.0), 0.5);
}

// Why: reversed colour limits are a legitimate way to flip a colour map.
#[test]
fn normalise_reversed_limits_invert_the_mapping() {
    assert_eq!(normalise(10.0, 10.0, 0.0), 0.0);
    assert_eq!(normalise(0.0, 10.0, 0.0), 1.0);
}

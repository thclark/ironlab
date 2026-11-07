use ironlab::prelude::*;

pub const TITLE: &str = "Image on a logarithmic frequency axis";
pub const DESCRIPTION: &str = "The short-time Fourier transform of the exponential sine sweep of the \
     [spectrogram on a logarithmic frequency axis](sweep_spectrogram.md), drawn as a scalar image. A Fourier \
     transform resolves frequencies in bins of equal width, so the rows of the image are evenly spaced in \
     frequency, and on a logarithmic axis each row is drawn between its own edges: the rows are tall at low \
     frequencies, where one bin spans a large fraction of a decade, and thin at high frequencies. This is why a \
     constant-Q analysis, whose bins are evenly spaced in the logarithm of frequency, is preferred for audio \
     measurements.";

/// The duration of the sweep in seconds.
const DURATION_S: f64 = 8.0;

/// The frequencies at which the sweep starts and ends, in hertz.
const START_HZ: f64 = 20.0;
const END_HZ: f64 = 20_000.0;

/// The power of each harmonic relative to the fundamental, in decibels, starting with the fundamental itself.
const HARMONICS_DB: [f64; 4] = [0.0, -25.0, -35.0, -45.0];

/// The sampling rate of the recording and the length of each transform, which together set the width of a bin.
const SAMPLE_RATE_HZ: f64 = 48_000.0;
const FRAME_LENGTH: usize = 1024;

/// The number of frames analysed over the sweep.
const FRAMES: usize = 120;

/// The noise floor of the measurement, in decibels relative to the fundamental.
const NOISE_FLOOR_DB: f64 = -70.0;

/// The frequency of the sweep at time `t` in seconds: it rises exponentially, by the same ratio every second.
fn sweep_hz(t: f64) -> f64 {
    START_HZ * (END_HZ / START_HZ).powf(t / DURATION_S)
}

/// The power in the bin centred on `f` hertz of a frame at time `t` seconds, in decibels relative to the
/// fundamental: each harmonic of the sweep spreads over about two bins, whatever its frequency, as a Hann window
/// spreads a tone.
fn power_db(t: f64, f: f64, bin_hz: f64) -> f64 {
    let fundamental = sweep_hz(t);
    let tones: f64 = HARMONICS_DB
        .iter()
        .enumerate()
        .map(|(k, level_db)| {
            let centre = (k + 1) as f64 * fundamental;
            10f64.powf(level_db / 10.0) * (-0.5 * ((f - centre) / bin_hz).powi(2)).exp()
        })
        .sum();
    10.0 * (tones + 10f64.powf(NOISE_FLOOR_DB / 10.0)).log10()
}

pub fn figure() -> Figure {
    // Bin k is centred on k times the bin width; bin 0 holds the mean of the signal, which a logarithmic axis
    // cannot place, so the image starts at bin 1 and ends just below half the sampling rate.
    let bin_hz = SAMPLE_RATE_HZ / FRAME_LENGTH as f64;
    let bins = FRAME_LENGTH / 2 - 1;
    let frame_s = DURATION_S / FRAMES as f64;
    let power = Matrix::from_fn(bins, FRAMES, |row, col| {
        let t = (col as f64 + 0.5) * frame_s;
        power_db(t, (row + 1) as f64 * bin_hz, bin_hz)
    });

    let mut fig = Figure::new()
        .size_mm(160.0, 90.0)
        .title("Short-time Fourier transform of an exponential sine sweep")
        .tag("image")
        .tag("log")
        .tag("signals")
        .tag("spectrogram")
        .parameter("dimensionality", "2D")
        .parameter("artists", 1)
        .parameter("data_points", 61_320)
        .parameter("has_legend", false);
    let mut ax = fig.axes(0, 0);
    // The rows count up from bin 1, so row 0 is placed at the lowest frequency, and the columns are the centres
    // of the frames.
    ax.scalar_image(power)
        .pixel_columns(0.5 * frame_s, DURATION_S - 0.5 * frame_s)
        .pixel_rows(bin_hz, bins as f64 * bin_hz);
    ax.color_scale()
        .colormap(Colormap::Magma)
        .limits(NOISE_FLOOR_DB, 0.0);
    ax.yscale(Scale::Log)
        .xlabel("Time $t$ (s)")
        .ylabel("Frequency $f$ (Hz)");
    fig
}

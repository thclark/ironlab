//! Images: image, indexed_image and mapped_image, and the pixel containers they take.

mod common;

use common::{
    artist, axes, data, has_error_at, image, image_figure, indexed_image, is_3d, mapped_image,
    parent,
};
use ironlab::ir::{
    Artist, ImagePlacement, IrError, IssueKind, NdArray, NdArrayElement, PixelRange, Projection,
    View3d,
};
use ironlab::prelude::*;

/// The bytes of a 2 by 3 true-colour image: a row of pure red, green and blue pixels
/// over a row of black, mid-grey and white.
const RGB: [u8; 18] = [
    255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 128, 128, 128, 255, 255, 255,
];

/// The bytes of a 2 by 2 image with alpha, in which every component is distinct enough
/// to show a misplaced channel.
const RGBA: [u8; 16] = [255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 10, 20, 30, 40];

/// The 2 by 3 true-colour image of `RGB`.
fn sample_pixels() -> Pixels {
    Pixels::from_rgb8(2, 3, &RGB).expect("18 bytes make 2 by 3 RGB pixels")
}

/// A 2 by 3 matrix of bytes counting from 0 in row-major order.
fn sample_bytes() -> ByteMatrix {
    ByteMatrix::from_fn(2, 3, |row, col| (row * 3 + col) as u8)
}

/// A 2 by 3 matrix of floats from 0 to 1 in steps of 0.2, in row-major order.
fn sample_floats() -> Matrix {
    Matrix::from_fn(2, 3, |row, col| (row * 3 + col) as f64 / 5.0)
}

/// Returns the placement of an image of any of the three kinds.
fn placement(fig: &Figure, id: NodeId) -> ImagePlacement {
    match artist(fig, id) {
        Artist::Image(a) => a.placement,
        Artist::IndexedImage(a) => a.placement,
        Artist::MappedImage(a) => a.placement,
        other => panic!("expected an image, found {other:?}"),
    }
}

// WHY: the scene compiler reads the components of the pixel in row `j` and column `i`
// at `(j * nx + i) * channels`, so the pixels must be stored as one 8-bit array of
// shape [rows, cols, channels] in row-major pixel order with the bytes exactly as
// given: a floating-point copy would multiply the memory of a photograph by eight, and
// a transposed or re-interleaved copy would draw a plausible but wrong picture. Three-
// and four-channel pixels are stored alike, since the last dimension tells them apart.
#[test]
fn image_stores_the_pixels_as_bytes_with_shape_rows_by_cols_by_channels() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let rgb = ax.image(&sample_pixels()).id();
    let rgba = ax.image(&Pixels::from_rgba8(2, 2, &RGBA).unwrap()).id();

    let stored = data(&fig, image(&fig, rgb).pixels);
    assert_eq!(stored.shape, vec![2, 3, 3]);
    assert_eq!(stored.element(), NdArrayElement::U8);
    assert_eq!(stored.as_u8(), Some(&RGB[..]));

    let stored = data(&fig, image(&fig, rgba).pixels);
    assert_eq!(stored.shape, vec![2, 2, 4]);
    assert_eq!(stored.element(), NdArrayElement::U8);
    assert_eq!(stored.as_u8(), Some(&RGBA[..]));
    assert!(fig.validate().is_valid());
}

// WHY: with no setters called, `image(&pixels)` alone must show the picture in a
// two-dimensional axes as MATLAB's `image` does: in the xy plane at the low end of z,
// with pixel centres at 0 to n − 1 (an absent range) and every out-of-range category
// lenient, which are the IR defaults the file format documents. The axes must stay
// two-dimensional, because a floor image is a planar plot, and the handle's identifier
// must name an artist of the kind the function is named after.
#[test]
fn every_image_kind_defaults_to_the_floor_plane_with_no_ranges_and_lenient_policies() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let plain = ax.image(&sample_pixels()).id();
    let indexed = ax.indexed_image(sample_bytes()).id();
    let mapped = ax.mapped_image(sample_floats()).id();

    for id in [plain, indexed, mapped] {
        let p = placement(&fig, id);
        assert_eq!(p.plane, ImagePlane::Xy { z: None }, "{id}");
        assert_eq!(p.columns, None, "{id}");
        assert_eq!(p.rows, None, "{id}");
        assert_eq!(artist(&fig, id).display_name(), None, "{id}");
        assert!(artist(&fig, id).visible(), "{id}");
        assert!(!is_3d(parent(&fig, id)), "{id}");
    }
    let transparent = [OutOfRange::Transparent; 3];
    let i = indexed_image(&fig, indexed);
    assert_eq!([i.below, i.above, i.non_finite], transparent);
    let m = mapped_image(&fig, mapped);
    assert_eq!([m.below, m.above, m.non_finite], transparent);
    assert!(fig.validate().is_valid());
}

// WHY: an indexed image looks each index up directly, so a byte index must be stored
// as the byte it is (exact, and a quarter of the size of a float) and a floating-point
// index as the float, which the compiler truncates when it draws; a silent conversion
// either way would clamp or widen the data. Both containers must give a two-dimensional
// array of shape [rows, cols] in row-major order, as a Matrix is laid out, and be
// accepted by reference and by value, so that a computed container need not be cloned
// to be plotted.
#[test]
fn indexed_image_stores_bytes_from_a_byte_matrix_and_floats_from_a_matrix() {
    let bytes = sample_bytes();
    let floats = sample_floats();
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let bytes_by_ref = ax.indexed_image(&bytes).id();
    let floats_by_ref = ax.indexed_image(&floats).id();
    let bytes_by_value = ax.indexed_image(sample_bytes()).id();
    let floats_by_value = ax.indexed_image(sample_floats()).id();

    let expected_bytes = NdArray::from_shape_u8(vec![2, 3], bytes.values().to_vec()).unwrap();
    let expected_floats = NdArray::from_shape(vec![2, 3], floats.values().to_vec()).unwrap();
    for id in [bytes_by_ref, bytes_by_value] {
        let stored = data(&fig, indexed_image(&fig, id).indices);
        assert_eq!(stored.element(), NdArrayElement::U8, "{id}");
        assert_eq!(stored, &expected_bytes, "{id}");
    }
    for id in [floats_by_ref, floats_by_value] {
        let stored = data(&fig, indexed_image(&fig, id).indices);
        assert_eq!(stored.element(), NdArrayElement::F64, "{id}");
        assert_eq!(stored, &expected_floats, "{id}");
    }
    assert!(fig.validate().is_valid());
}

// WHY: a mapped image maps a byte as the number it denotes and a float as itself, so
// the element type must follow the container as for an indexed image; and the values
// must reach the table untouched, including NaN, because the out-of-range policies
// decide at drawing time what a non-finite pixel becomes, not the builder.
#[test]
fn mapped_image_stores_bytes_from_a_byte_matrix_and_floats_from_a_matrix() {
    let bytes = sample_bytes();
    let floats = Matrix::from_rows(&[[0.0, 0.5, f64::NAN], [1.0, -0.5, 1.5]]);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let bytes_by_ref = ax.mapped_image(&bytes).id();
    let floats_by_ref = ax.mapped_image(&floats).id();
    let bytes_by_value = ax.mapped_image(sample_bytes()).id();
    let floats_by_value = ax.mapped_image(floats.clone()).id();

    let expected_bytes = NdArray::from_shape_u8(vec![2, 3], bytes.values().to_vec()).unwrap();
    let expected_floats = NdArray::from_shape(vec![2, 3], floats.values().to_vec()).unwrap();
    for id in [bytes_by_ref, bytes_by_value] {
        let stored = data(&fig, mapped_image(&fig, id).values);
        assert_eq!(stored.element(), NdArrayElement::U8, "{id}");
        assert_eq!(stored, &expected_bytes, "{id}");
    }
    for id in [floats_by_ref, floats_by_value] {
        let stored = data(&fig, mapped_image(&fig, id).values);
        assert_eq!(stored.element(), NdArrayElement::F64, "{id}");
        assert_eq!(stored, &expected_floats, "{id}");
    }
    assert!(fig.validate().is_valid());
}

// WHY: Pixels is the container that users fill from decoded image files and from their
// own byte buffers; the byte constructors must keep the bytes exactly as given (not
// pass them through floats, which is slow at megapixels and needless at 8 bits) and
// report the rows, columns and channel count from which the image array is shaped.
#[test]
fn pixels_from_bytes_keep_the_bytes_and_report_their_shape() {
    let rgb = Pixels::from_rgb8(2, 3, &RGB).unwrap();
    assert_eq!((rgb.rows(), rgb.cols(), rgb.channels()), (2, 3, 3));
    assert_eq!(rgb.bytes(), &RGB[..]);

    let rgba = Pixels::from_rgba8(2, 2, &RGBA).unwrap();
    assert_eq!((rgba.rows(), rgba.cols(), rgba.channels()), (2, 2, 4));
    assert_eq!(rgba.bytes(), &RGBA[..]);
}

// WHY: the byte count is the one consistency a bytes constructor can check, and a
// count that does not match the shape (typically the bytes of an RGB image passed to
// from_rgba8, or a buffer with padding at the end of each row) must be an Error rather
// than a panic, because the bytes usually come from a file at run time; it is the same
// InvalidShape that Matrix::from_vec reports, so one match arm handles both. A shape
// whose byte count overflows cannot be honoured either and must be refused the same
// way rather than panic on the multiplication in a debug build, whichever factor
// overflows: the pixel count, or the pixel count times the channels when the pixel
// count itself fits. An image with no pixels, however, is exactly matched by no bytes.
#[test]
fn pixels_from_bytes_refuse_a_byte_count_that_does_not_match_the_shape() {
    let invalid_shape = |result: Result<Pixels, Error>| {
        matches!(result, Err(Error::Ir(IrError::InvalidShape { .. })))
    };
    assert!(invalid_shape(Pixels::from_rgb8(2, 3, &RGB[..17])));
    assert!(invalid_shape(Pixels::from_rgb8(2, 3, &[0; 19])));
    // The bytes of a 2 by 2 RGB image are too few for RGBA, and RGBA bytes too many
    // for RGB.
    assert!(invalid_shape(Pixels::from_rgba8(2, 2, &[0; 12])));
    assert!(invalid_shape(Pixels::from_rgb8(2, 2, &[0; 16])));
    // The pixel count overflows.
    assert!(invalid_shape(Pixels::from_rgb8(usize::MAX, 2, &[0; 6])));
    assert!(invalid_shape(Pixels::from_rgba8(usize::MAX, 2, &[0; 8])));
    // The pixel count fits, but the byte count does not.
    assert!(invalid_shape(Pixels::from_rgb8(usize::MAX / 2, 1, &[0; 3])));
    assert!(invalid_shape(Pixels::from_rgba8(
        usize::MAX / 3,
        1,
        &[0; 4]
    )));
    assert!(Pixels::from_rgb8(0, 3, &[]).is_ok());
}

// WHY: rgb_from_fn is how computed pictures (domain colourings, fractals) are built
// from Colors without the user quantising by hand. Every component must be quantised
// to the nearest of 256 levels (so 0.5 is 128, not 127 by truncation), clamped rather
// than wrapped when a computation strays outside 0 to 1, laid out in row-major order
// as Matrix::from_fn lays values out, and the alpha of the Color ignored so that an
// opaque picture has three channels. The closure constructors are infallible like
// Matrix::from_fn, so a non-finite component cannot be refused and must quantise to a
// definite byte, 0, rather than to whatever a cast of NaN happens to give.
#[test]
fn rgb_from_fn_quantises_components_in_row_major_order_and_drops_alpha() {
    let pixels = Pixels::rgb_from_fn(2, 2, |row, col| match (row, col) {
        (0, 0) => Color::rgb(0.5, 0.0, 1.0),
        (0, 1) => Color::rgb(1.2, -0.3, 0.25),
        (1, 0) => Color::rgba(1.0, 1.0, 1.0, 0.5),
        _ => Color::rgb(f32::NAN, 0.5, 0.5),
    });
    assert_eq!((pixels.rows(), pixels.cols(), pixels.channels()), (2, 2, 3));
    assert_eq!(
        pixels.bytes(),
        &[128, 0, 255, 255, 0, 64, 255, 255, 255, 0, 128, 128]
    );
}

// WHY: rgba_from_fn exists for pictures that fade out (the gallery fades a domain
// colouring outside a disc), so the alpha of each Color must reach a fourth channel,
// quantised like the colour components, with pixels four bytes apart; an opaque Color
// must give a fully opaque alpha byte.
#[test]
fn rgba_from_fn_keeps_alpha_as_a_fourth_channel() {
    let pixels = Pixels::rgba_from_fn(1, 3, |_, col| match col {
        0 => Color::rgba(1.0, 0.0, 0.0, 0.5),
        1 => Color::rgb(0.0, 1.0, 0.0),
        _ => Color::rgba(0.0, 0.0, 1.0, 0.0),
    });
    assert_eq!((pixels.rows(), pixels.cols(), pixels.channels()), (1, 3, 4));
    assert_eq!(
        pixels.bytes(),
        &[255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 255, 0]
    );
}

// WHY: from_planes takes the form in which computed colour data usually exists (one
// Matrix per component, as MATLAB's `cat(3, r, g, b)` takes) and must quantise and
// clamp each component exactly as rgb_from_fn does, pixel by pixel in row-major
// order, so that the two ways of building the same picture agree byte for byte.
#[test]
fn pixels_from_planes_quantise_and_clamp_each_component() {
    let r = Matrix::from_rows(&[[0.0, 0.5], [1.0, 1.2]]);
    let g = Matrix::from_rows(&[[1.0, 0.0], [-0.3, 0.5]]);
    let b = Matrix::from_rows(&[[0.5, 1.0], [0.0, 0.0]]);
    let pixels = Pixels::from_planes(&r, &g, &b).unwrap();
    assert_eq!((pixels.rows(), pixels.cols(), pixels.channels()), (2, 2, 3));
    assert_eq!(
        pixels.bytes(),
        &[0, 255, 128, 128, 0, 255, 255, 0, 0, 255, 128, 0]
    );
}

// WHY: planes of different shapes cannot form pixels, and computed planes are run-time
// data, so the mismatch is an Error and never a panic: PlaneShapeMismatch, whose
// expected and found shapes let the message say which plane is wrong. A 2 by 3 and a
// 3 by 2 plane have the same number of values, so an implementation that compared only
// lengths would interleave them into a scrambled picture without noticing; each
// argument position is tried, because a check on the first two planes alone would miss
// the third.
#[test]
fn pixels_from_planes_refuse_planes_of_different_shapes() {
    let wide = Matrix::zeros(2, 3);
    let tall = Matrix::zeros(3, 2);
    let shape_mismatch =
        |result: Result<Pixels, Error>| matches!(result, Err(Error::PlaneShapeMismatch { .. }));
    assert!(shape_mismatch(Pixels::from_planes(&tall, &wide, &wide)));
    assert!(shape_mismatch(Pixels::from_planes(&wide, &tall, &wide)));
    assert!(shape_mismatch(Pixels::from_planes(&wide, &wide, &tall)));
    assert!(Pixels::from_planes(&wide, &wide, &wide).is_ok());
}

// WHY: a NaN or infinite component has no byte, and clamping it silently would hide a
// failed computation (an infinity would clamp to full intensity and a NaN to whatever
// the cast gives). The rule that a pixel with a non-finite component is transparent
// applies to floating-point image arrays in the IR, not to the byte container, so
// from_planes must refuse such a component in any plane with NonFiniteComponent, which
// names the row, the column and the channel (0 for red, 1 for green, 2 for blue) so
// that the message can point at the pixel.
#[test]
fn pixels_from_planes_refuse_a_non_finite_component() {
    let good = Matrix::from_rows(&[[0.5, 0.5]]);
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let plane = Matrix::from_rows(&[[0.5, bad]]);
        let planes = [
            (Pixels::from_planes(&plane, &good, &good), 0),
            (Pixels::from_planes(&good, &plane, &good), 1),
            (Pixels::from_planes(&good, &good, &plane), 2),
        ];
        for (result, channel) in planes {
            assert!(
                matches!(
                    result,
                    Err(Error::NonFiniteComponent { row: 0, col: 1, channel: c }) if c == channel
                ),
                "{bad} in channel {channel}"
            );
        }
    }
}

// WHY: with_alpha is how a mask computed as a Matrix (the usual form) is attached to
// colour data; it must interleave each pixel's alpha byte after its colour bytes rather
// than append a plane, because the image array holds the components of a pixel
// together, and the colour bytes must be unchanged.
#[test]
fn with_alpha_adds_a_fourth_channel_to_three_channel_pixels() {
    let alpha = Matrix::from_rows(&[[1.0, 0.5, 0.0], [0.0, 0.5, 1.0]]);
    let pixels = sample_pixels().with_alpha(&alpha).unwrap();
    assert_eq!((pixels.rows(), pixels.cols(), pixels.channels()), (2, 3, 4));
    assert_eq!(
        pixels.bytes(),
        &[
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 0, 0, 0, 0, 128, 128, 128, 128, 255, 255,
            255, 255
        ]
    );
}

// WHY: with_alpha on pixels that already have alpha (for example RGBA bytes from a file
// whose mask is recomputed) must replace the fourth channel rather than add a fifth or
// refuse, keeping the colour bytes; and an alpha outside 0 to 1 is clamped as every
// other component is, so a fade computed slightly past 1 saturates instead of failing.
#[test]
fn with_alpha_replaces_an_existing_alpha_channel() {
    let alpha = Matrix::from_rows(&[[1.0, 1.5], [0.0, 0.5]]);
    let pixels = Pixels::from_rgba8(2, 2, &RGBA)
        .unwrap()
        .with_alpha(&alpha)
        .unwrap();
    assert_eq!((pixels.rows(), pixels.cols(), pixels.channels()), (2, 2, 4));
    assert_eq!(
        pixels.bytes(),
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 0, 10, 20, 30, 128
        ]
    );
}

// WHY: as for from_planes, an alpha plane of another shape (including one of the same
// length) or with a non-finite value cannot be attached, and both are Errors rather
// than panics: a PlaneShapeMismatch that expects the pixels' shape and finds the
// alpha's, and a NonFiniteComponent at the pixel's row and column in channel 3, the
// alpha. A plane of the right shape is accepted, so the refusals are not a blanket
// failure.
#[test]
fn with_alpha_refuses_a_shape_mismatch_and_a_non_finite_alpha() {
    assert!(matches!(
        sample_pixels().with_alpha(&Matrix::zeros(3, 2)),
        Err(Error::PlaneShapeMismatch {
            expected: [2, 3],
            found: [3, 2]
        })
    ));
    assert!(matches!(
        sample_pixels().with_alpha(&Matrix::zeros(2, 2)),
        Err(Error::PlaneShapeMismatch {
            expected: [2, 3],
            found: [2, 2]
        })
    ));
    let nan = Matrix::from_fn(
        2,
        3,
        |row, col| {
            if (row, col) == (1, 2) { f64::NAN } else { 1.0 }
        },
    );
    assert!(matches!(
        sample_pixels().with_alpha(&nan),
        Err(Error::NonFiniteComponent {
            row: 1,
            col: 2,
            channel: 3
        })
    ));
    assert!(sample_pixels().with_alpha(&Matrix::zeros(2, 3)).is_ok());
}

// WHY: a ByteMatrix is the byte counterpart of Matrix and feeds indexed and mapped
// images with the same row-major layout (rows are y, columns are x); if from_fn filled
// column-major, every byte image would be transposed relative to its float twin, and
// indexing by (row, col) must agree with the layout.
#[test]
fn byte_matrix_from_fn_fills_row_major() {
    let m = ByteMatrix::from_fn(2, 3, |row, col| (10 * row + col) as u8);
    assert_eq!((m.rows(), m.cols()), (2, 3));
    assert_eq!(m.values(), &[0, 1, 2, 10, 11, 12]);
    assert_eq!(m[(0, 2)], 2);
    assert_eq!(m[(1, 0)], 10);
}

// WHY: from_rows is the literal constructor used in examples and tests, zeros with
// IndexMut is how loops fill a matrix MATLAB style, and into_values hands the bytes on
// without a copy; from_rows must keep row order and agree with from_fn, compared as
// whole matrices as users and tests compare them, and every constructor must report
// the shape.
#[test]
fn byte_matrix_from_rows_and_zeros_agree_with_from_fn() {
    let m = ByteMatrix::from_rows(&[[0u8, 1, 2], [10, 11, 12]]);
    assert_eq!((m.rows(), m.cols()), (2, 3));
    assert_eq!(
        m,
        ByteMatrix::from_fn(2, 3, |row, col| (10 * row + col) as u8)
    );
    assert_eq!(m.into_values(), vec![0, 1, 2, 10, 11, 12]);

    let mut z = ByteMatrix::zeros(2, 2);
    assert_eq!((z.rows(), z.cols()), (2, 2));
    assert_eq!(z.values(), &[0; 4]);
    z[(0, 1)] = 5;
    assert_eq!(z.values(), &[0, 5, 0, 0]);
}

// WHY: ragged rows cannot form a matrix; as for Matrix, the documented behaviour is a
// panic with the same message, because literal rows are a programming error rather
// than a run-time condition, and a user should meet one rule for both containers.
#[test]
#[should_panic(expected = "matrix rows have different lengths")]
fn byte_matrix_from_rows_panics_on_ragged_rows() {
    let _ = ByteMatrix::from_rows(&[vec![0u8, 1], vec![2]]);
}

// WHY: as for Matrix, an index past the last column must panic rather than read the
// next row: on a 2 by 2 matrix the flat index of (0, 2) is a valid position in the
// bytes, so only an explicit column check catches it.
#[test]
#[should_panic(expected = "out of range")]
fn byte_matrix_index_past_the_last_column_panics() {
    let m = ByteMatrix::zeros(2, 2);
    let _ = m[(0, 2)];
}

// WHY: see byte_matrix_index_past_the_last_column_panics; writes must be checked in the
// same way, or a loop with an off-by-one column bound silently corrupts the next row.
#[test]
#[should_panic(expected = "out of range")]
fn byte_matrix_index_mut_past_the_last_column_panics() {
    let mut m = ByteMatrix::zeros(2, 2);
    m[(0, 2)] = 1;
}

// WHY: users keep the pixel containers in their own structs, decode an image on one
// thread and plot it on another, log them and compare them in their tests; that needs
// Clone, Debug, PartialEq, Send, Sync and 'static, which one non-thread-safe or
// non-comparable field would silently break, as it would for Error.
#[test]
fn image_containers_are_clonable_comparable_and_thread_safe() {
    fn value_type<T: Clone + std::fmt::Debug + PartialEq + Send + Sync + 'static>() {}
    value_type::<Pixels>();
    value_type::<ByteMatrix>();
    value_type::<ImageValues>();
}

/// The prelude test lives in a module whose only import is the glob, so that an
/// explicit `use ironlab::ir::…` elsewhere in this file cannot stand in for a missing
/// prelude export without anyone noticing.
mod prelude_only {
    use ironlab::prelude::*;

    // WHY: `use ironlab::prelude::*` is the documented single import for building
    // figures; the image containers, the plane and policy enums (needed for every
    // non-default placement or policy) and the handle types (which appear in the
    // signatures of helpers that style images) must all come with it, or an image
    // script needs a second import from the ir crate that no other plot type needs.
    // Naming each type through the prelude alone is what this test checks.
    #[test]
    fn the_prelude_exports_the_image_types_and_handles() {
        fn place(image: &mut ImageMut<'_>) {
            image.pixel_columns(0.0, 1.0);
        }
        fn classify(image: &mut IndexedImageMut<'_>) {
            image.above(OutOfRange::Clamp);
        }
        fn map(image: &mut MappedImageMut<'_>) {
            image.plane(ImagePlane::Xy { z: None });
        }
        let bytes = ByteMatrix::zeros(1, 1);
        let indices: ImageValues = ImageValues::from(&bytes);
        let pixels: Pixels = Pixels::rgb_from_fn(1, 1, |_, _| Color::WHITE);
        let mut fig = Figure::new();
        let mut ax = fig.axes(0, 0);
        place(&mut ax.image(&pixels));
        classify(&mut ax.indexed_image(indices));
        map(&mut ax.mapped_image(Matrix::zeros(1, 1)));
        assert!(fig.validate().is_valid());
    }
}

// WHY: pixel_columns and pixel_rows place the pixel centres along the two axes of the
// plane (MATLAB's XData and YData) and are the only way to put an image over real
// coordinates, so every handle must have both; each must reach its own field and leave
// the other absent, and a range whose last centre is less than the first must be
// stored as given, because that is how an image is mirrored. display_name feeds the
// legend as for every other artist.
#[test]
fn pixel_columns_and_pixel_rows_set_the_pixel_centres_on_every_kind() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let plain = ax
        .image(&sample_pixels())
        .pixel_columns(-1.5, 1.5)
        .pixel_rows(2.0, 0.0)
        .display_name("photo")
        .id();
    let indexed = ax
        .indexed_image(sample_bytes())
        .pixel_columns(-1.5, 1.5)
        .pixel_rows(2.0, 0.0)
        .id();
    let mapped = ax
        .mapped_image(sample_floats())
        .pixel_columns(-1.5, 1.5)
        .pixel_rows(2.0, 0.0)
        .id();
    let columns_only = ax.image(&sample_pixels()).pixel_columns(0.0, 10.0).id();
    let rows_only = ax.mapped_image(sample_floats()).pixel_rows(-2.0, -1.0).id();
    let range = |first, last| Some(PixelRange { first, last });

    for id in [plain, indexed, mapped] {
        let p = placement(&fig, id);
        assert_eq!(p.columns, range(-1.5, 1.5), "{id}");
        assert_eq!(p.rows, range(2.0, 0.0), "{id}");
        assert_eq!(p.plane, ImagePlane::Xy { z: None }, "{id}");
    }
    assert_eq!(image(&fig, plain).display_name, Some(Text::new("photo")));
    let p = placement(&fig, columns_only);
    assert_eq!((p.columns, p.rows), (range(0.0, 10.0), None));
    let p = placement(&fig, rows_only);
    assert_eq!((p.columns, p.rows), (None, range(-2.0, -1.0)));
    assert!(fig.validate().is_valid());
}

// WHY: the xz and yz planes are the walls of a three-dimensional axes, so placing an
// image there is a three-dimensional plot; like surf, the setter on every handle must
// convert a two-dimensional axes to 3D with the default view rather than leave the
// user with a ThreeDArtistInTwoDAxes error, and the plane with its offset must be
// stored as given, an absent offset meaning the low end of the third axis.
#[test]
fn wall_planes_promote_the_axes_to_3d_with_the_default_view_on_every_kind() {
    let xz = ImagePlane::Xz { y: Some(0.5) };
    let yz = ImagePlane::Yz { x: None };
    let mut fig = Figure::new().tiles(2, 3);
    let image_xz = fig.axes(0, 0).image(&sample_pixels()).plane(xz).id();
    let indexed_xz = fig.axes(0, 1).indexed_image(sample_bytes()).plane(xz).id();
    let mapped_xz = fig.axes(0, 2).mapped_image(sample_floats()).plane(xz).id();
    let image_yz = fig.axes(1, 0).image(&sample_pixels()).plane(yz).id();
    let indexed_yz = fig.axes(1, 1).indexed_image(sample_bytes()).plane(yz).id();
    let mapped_yz = fig.axes(1, 2).mapped_image(sample_floats()).plane(yz).id();

    let placed = [
        (image_xz, xz),
        (indexed_xz, xz),
        (mapped_xz, xz),
        (image_yz, yz),
        (indexed_yz, yz),
        (mapped_yz, yz),
    ];
    for (id, plane) in placed {
        assert_eq!(placement(&fig, id).plane, plane, "{id}");
        assert_eq!(
            parent(&fig, id).projection,
            Projection::ThreeD {
                view3d: View3d::default()
            },
            "{id}"
        );
    }
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
}

// WHY: the xy plane is the floor of a 3D axes, but in a 2D axes it is the only plane
// and its height is ignored when drawing; setting an xy plane with an offset must
// store it (an axes3 call afterwards would then use it) without converting the axes,
// or every image in a plain 2D figure would turn 3D, and validation must accept it
// silently. Conversely, the xy plane must not demote an axes that is already
// three-dimensional, where it is the floor.
#[test]
fn the_floor_plane_with_an_offset_leaves_a_2d_axes_two_dimensional() {
    let floor = ImagePlane::Xy { z: Some(1.0) };
    let mut fig = Figure::new().tiles(1, 2);
    let mut ax = fig.axes(0, 0);
    let plain = ax.image(&sample_pixels()).plane(floor).id();
    let indexed = ax.indexed_image(sample_bytes()).plane(floor).id();
    let mapped = ax.mapped_image(sample_floats()).plane(floor).id();
    let on_the_floor_of_3d = fig
        .axes3(0, 1)
        .image(&sample_pixels())
        .plane(ImagePlane::Xy { z: None })
        .id();

    for id in [plain, indexed, mapped] {
        assert_eq!(placement(&fig, id).plane, floor, "{id}");
        assert!(!is_3d(parent(&fig, id)), "{id}");
    }
    assert!(is_3d(parent(&fig, on_the_floor_of_3d)));
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// WHY: a user who has set the camera of a 3D axes and then adds a wall image must keep
// that camera, zoom and pan included; plane() converts only a 2D axes, through the same
// path as surf, so the whole projection must be exactly what it was.
#[test]
fn a_wall_plane_keeps_the_view_of_an_axes_that_is_already_3d() {
    let mut fig = Figure::new();
    let axes_id = fig.axes3(0, 0).view(45.0, 10.0).id();
    let before = axes(&fig, axes_id).projection;
    assert_eq!(
        before,
        Projection::ThreeD {
            view3d: View3d {
                azimuth_deg: 45.0,
                elevation_deg: 10.0,
                ..View3d::default()
            }
        }
    );
    let id = fig
        .axes(0, 0)
        .mapped_image(sample_floats())
        .plane(ImagePlane::Yz { x: Some(-1.0) })
        .id();
    assert_eq!(parent(&fig, id).projection, before);
}

// WHY: the three out-of-range categories are independent properties (the design lets
// NaN be tolerated while values outside the range are refused, or the reverse), so
// each setter must write only its own field and leave the others at their default; a
// Color must become the fixed-colour policy, which is the common lenient choice, and
// the enum variants must pass through unchanged. Both mapped kinds have the setters.
#[test]
fn out_of_range_policies_map_to_their_ir_fields_on_both_mapped_kinds() {
    let red = Color::rgb(1.0, 0.0, 0.0);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let indexed = ax
        .indexed_image(sample_bytes())
        .below(red)
        .above(OutOfRange::Clamp)
        .non_finite(OutOfRange::Strict)
        .display_name("classes")
        .id();
    let mapped = ax
        .mapped_image(sample_floats())
        .below(OutOfRange::Strict)
        .above(red)
        .non_finite(OutOfRange::Clamp)
        .display_name("$\\phi$")
        .id();
    let only_below = ax.mapped_image(sample_floats()).below(red).id();
    let only_above = ax.indexed_image(sample_bytes()).above(red).id();

    let fixed = OutOfRange::Rgba { color: red };
    let i = indexed_image(&fig, indexed);
    assert_eq!(
        [i.below, i.above, i.non_finite],
        [fixed, OutOfRange::Clamp, OutOfRange::Strict]
    );
    assert_eq!(i.display_name, Some(Text::new("classes")));
    let m = mapped_image(&fig, mapped);
    assert_eq!(
        [m.below, m.above, m.non_finite],
        [OutOfRange::Strict, fixed, OutOfRange::Clamp]
    );
    assert_eq!(m.display_name, Some(Text::new("$\\phi$")));
    let m = mapped_image(&fig, only_below);
    assert_eq!(
        [m.below, m.above, m.non_finite],
        [fixed, OutOfRange::Transparent, OutOfRange::Transparent]
    );
    let i = indexed_image(&fig, only_above);
    assert_eq!(
        [i.below, i.above, i.non_finite],
        [OutOfRange::Transparent, fixed, OutOfRange::Transparent]
    );
}

// WHY: a strict category is the one policy that can make a figure invalid, so the three
// facts that belong to the facade must hold: the builder stores NaN data under a strict
// policy without panicking, validate attributes the PixelOutOfRange error to the
// identifier the handle returned (so the user learns which image is wrong), and the
// default policy is lenient, so the same data with no policy set validates. Which
// pixels each category covers is the validator's contract, tested in the ir crate.
#[test]
fn a_strict_policy_is_reported_at_the_handles_id_and_the_default_policy_is_lenient() {
    let with_nan = Matrix::from_fn(
        2,
        3,
        |row, col| {
            if (row, col) == (0, 1) { f64::NAN } else { 0.5 }
        },
    );
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let strict = ax
        .mapped_image(&with_nan)
        .non_finite(OutOfRange::Strict)
        .id();
    let lenient = ax.mapped_image(&with_nan).id();

    let report = fig.validate();
    assert!(
        has_error_at(&report, IssueKind::PixelOutOfRange, strict),
        "{report:?}"
    );
    assert!(
        !report
            .errors
            .iter()
            .any(|issue| issue.node == Some(lenient)),
        "{report:?}"
    );
}

// WHY: the builder never panics because of inconsistent input, so a pixel centre or a
// plane offset that is not finite must be stored as given and reported by validate as
// InvalidImagePlacement against the identifier the handle returned, so that the user
// learns which image is wrong. The finer rules of placement (coincident centres and
// the one-pixel exception) are the validator's contract, tested in the ir crate.
#[test]
fn non_finite_pixel_centres_and_offsets_are_reported_by_validate_not_a_panic() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let nan_centre = ax.image(&sample_pixels()).pixel_columns(f64::NAN, 1.0).id();
    let infinite_offset = ax
        .mapped_image(sample_floats())
        .plane(ImagePlane::Xy {
            z: Some(f64::INFINITY),
        })
        .id();

    let report = fig.validate();
    for id in [nan_centre, infinite_offset] {
        assert!(
            has_error_at(&report, IssueKind::InvalidImagePlacement, id),
            "{id}: {report:?}"
        );
    }
}

// WHY: the shared image fixture uses the whole image API (a mirrored photograph with a
// legend entry, a classified map with explicit centres and strict and clamped policies,
// and a field holding a NaN on a wall of a 3D axes with a fixed colour for it and
// manual colour limits), as the gallery and users do; it must validate with neither
// errors nor warnings, or the facade's setters would produce an IR that export
// refuses, and the persistence tests would be round-tripping a figure nobody could
// save usefully.
#[test]
fn one_image_of_each_kind_built_through_the_facade_validates_with_no_issues() {
    let report = image_figure().validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

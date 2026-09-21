//! Tests of the image orientation entry, of the photograph that it draws and of the credit that the photograph needs.
//!
//! WHY: IronLAB deliberately has no axis-direction or matrix-orientation property for images. The explicit pixel
//! ranges are the only way to flip, stretch and shift an image, and the image orientation entry is where the
//! documentation shows a reader how to do so. If the entry stopped demonstrating one of the three placements, the
//! documentation would silently stop answering the question that the entry exists to answer. The photograph is the
//! work of another author, who released it under a licence that requires attribution. The credit is therefore a legal
//! obligation of the repository and of every published page that shows the photograph, not decoration, and it is
//! tested as such.

mod common;

use std::fs;
use std::path::PathBuf;

use common::link_targets;
use ironlab::ir::{Artist, Axes, Figure, Image, NdArray, PixelRange, Projection};
use ironlab_gallery::docs::{entry_markdown, index_markdown};
use ironlab_gallery::{GalleryEntry, find};

/// The slug of the entry under test.
const SLUG: &str = "image_orientation";

/// The number of pixels along each side of the photograph.
const SIDE: usize = 256;

/// The page of the original photograph on Wikimedia Commons.
const SOURCE_URL: &str = "https://commons.wikimedia.org/wiki/File:011_Plains_zebra_eye_close-up_in_Etosha_National_Park_Photo_by_Giles_Laurent.jpg";

/// The author of the original photograph.
const AUTHOR: &str = "Giles Laurent";

/// The title of the original photograph.
const WORK_TITLE: &str = "Plains zebra eye close-up in Etosha National Park";

/// The name of the licence of the original photograph.
const LICENCE_NAME: &str = "CC BY-SA 4.0";

/// The address of the licence of the original photograph, without the trailing slash that Creative Commons appends to
/// it. Both forms address the same page, so the tests accept either.
const LICENCE_URL: &str = "https://creativecommons.org/licenses/by-sa/4.0";

/// The words that describe how the asset differs from the original photograph. The licence requires an adapted work
/// to say that it was modified, and these two modifications are the ones that were made.
const MODIFICATIONS: [&str; 2] = ["cropped", "downsampled"];

/// The least fraction by which a pitch must differ from one data unit for a reader to see, from the tick labels of
/// the axes, that the image was stretched.
const VISIBLE_STRETCH: f64 = 0.1;

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn asset_path() -> PathBuf {
    assets_dir().join("zebra_eye.png")
}

fn entry() -> GalleryEntry {
    find(SLUG).unwrap_or_else(|| panic!("the gallery has no entry with the slug {SLUG:?}"))
}

fn figure() -> Figure {
    (entry().build)().into_ir()
}

/// Returns the axes of the figure ordered by row and then by column, which is the order in which a reader sees them.
fn axes_in_reading_order(figure: &Figure) -> Vec<&Axes> {
    let mut axes: Vec<&Axes> = figure.axes.iter().collect();
    axes.sort_by_key(|a| (a.cell.row, a.cell.col));
    axes
}

/// Returns the only artist of an axes, which must be a true-colour image. A photograph is true-colour data, and any
/// second artist would distract from the comparison of the placements.
fn only_image(axes: &Axes) -> &Image {
    match axes.artists.as_slice() {
        [Artist::Image(image)] => image,
        other => panic!(
            "the axes in column {} must hold exactly one true-colour image, but it holds {} artists: {other:?}",
            axes.cell.col,
            other.len()
        ),
    }
}

/// Returns the three images of the entry in reading order, after checking that the figure is one row of three
/// two-dimensional axes.
fn three_images(figure: &Figure) -> [&Image; 3] {
    assert_eq!(
        (figure.layout.rows, figure.layout.cols),
        (1, 3),
        "the three placements are compared side by side in one row"
    );
    let axes = axes_in_reading_order(figure);
    assert_eq!(axes.len(), 3, "one axes per placement");
    for a in &axes {
        assert!(
            matches!(a.projection, Projection::TwoD),
            "the axes in column {} is not two-dimensional",
            a.cell.col
        );
    }
    [
        only_image(axes[0]),
        only_image(axes[1]),
        only_image(axes[2]),
    ]
}

fn pixels<'a>(figure: &'a Figure, image: &Image) -> &'a NdArray {
    figure
        .data
        .get(&image.pixels)
        .expect("the pixels of the image are in the data store of the figure")
}

/// Returns the distance between the centres of adjacent pixels that a range gives to `SIDE` pixels, which is negative
/// when the range mirrors the image.
fn pitch(range: PixelRange) -> f64 {
    (range.last - range.first) / (SIDE - 1) as f64
}

/// Returns whether a range is absent or spells out the default placement, with centres at 0 to `SIDE` − 1.
fn is_default(range: Option<PixelRange>) -> bool {
    range.is_none_or(|r| r.first == 0.0 && r.last == (SIDE - 1) as f64)
}

/// Returns whether a text names a modification, whatever the case of its first letter.
fn mentions(text: &str, modification: &str) -> bool {
    text.to_lowercase().contains(modification)
}

/// Returns whether a link target is the address of the licence, with or without a trailing slash.
fn is_licence_link(target: &str) -> bool {
    target.strip_suffix('/').unwrap_or(target) == LICENCE_URL
}

/// Decodes the asset to one byte per pixel, in the row order of the file (row 0 is the top of the photograph).
fn decoded_asset() -> image::GrayImage {
    let path = asset_path();
    image::open(&path)
        .unwrap_or_else(|e| panic!("cannot decode {}: {e}", path.display()))
        .into_luma8()
}

/// WHY: the entry teaches by comparison, and a comparison is only valid when the placement is the single thing that
/// differs between the tiles. Each tile must therefore draw the same pixels, in the same order, as one true-colour
/// image. An entry that made the second tile upright by reversing the rows of the pixel data, instead of by placing
/// them with a descending range, would teach exactly the workaround that the pixel ranges exist to make unnecessary.
/// The pixels are expected as bytes with three components because the facade stores true-colour pixels as bytes and
/// an opaque photograph has no use for an alpha component.
#[test]
fn every_tile_draws_the_same_pixels() {
    let figure = figure();
    let images = three_images(&figure);
    let reference = pixels(&figure, images[0])
        .as_u8()
        .expect("the photograph is stored as bytes");
    for (col, image) in images.iter().enumerate() {
        let array = pixels(&figure, image);
        assert_eq!(
            array.shape,
            vec![SIDE, SIDE, 3],
            "the pixel array of tile {col} has the wrong shape"
        );
        assert!(
            array.as_u8() == Some(reference),
            "tile {col} draws different pixels from tile 0, so the tiles differ in more than their placement"
        );
        assert!(image.visible, "tile {col} hides its image");
    }
}

/// WHY: the first tile shows what a reader gets without asking for anything: with neither range given, row 0 of the
/// array lies at y = 0, at the bottom of an axes whose y coordinate increases upwards, and the photograph appears
/// upside down. This is the behaviour that prompts the question which the entry answers, so the tile must leave both
/// ranges unset rather than spell out ranges that happen to equal the defaults.
#[test]
fn first_tile_uses_the_default_placement() {
    let figure = figure();
    let placement = three_images(&figure)[0].placement;
    assert_eq!(
        placement.columns, None,
        "the first tile must not place its columns"
    );
    assert_eq!(
        placement.rows, None,
        "the first tile must not place its rows"
    );
}

/// WHY: the second tile shows the smallest change that makes a photograph upright: the default row range, written
/// in the opposite direction. It must change nothing else, so that a reader can attribute the difference from the
/// first tile to the direction of the row range alone. The default placement puts the row centres at 0 to `SIDE` − 1,
/// so the simple flip in the y direction is the range from `SIDE` − 1 to 0: it has a pitch of exactly −1, and the
/// image covers exactly the region of the axes that it covers in the first tile. Any other descending range would
/// also stretch or shift the image, and the reader could no longer tell which part of the change made the photograph
/// upright. The expectation follows from the documented default placement, not from the code of the entry. The
/// columns must keep the default placement, whether the entry leaves them unset or spells the default out.
#[test]
fn second_tile_flips_the_rows_and_changes_nothing_else() {
    let figure = figure();
    let placement = three_images(&figure)[1].placement;
    let rows = placement
        .rows
        .expect("the second tile places its rows explicitly");
    assert!(
        rows.first > rows.last,
        "the row range of the second tile must descend so that row 0 is at the top, but it runs from {} to {}",
        rows.first,
        rows.last
    );
    let top = (SIDE - 1) as f64;
    assert!(
        rows.first == top && rows.last == 0.0,
        "the second tile must be a simple flip in the y direction: the default rows reversed in place, from {top} to \
         0, with a pitch of exactly -1, so that the image covers the same region as in the first tile and the \
         direction of the range is the only difference. Its rows run from {} to {}, with a pitch of {}",
        rows.first,
        rows.last,
        pitch(rows)
    );
    assert!(
        is_default(placement.columns),
        "the second tile must neither mirror, stretch nor shift the columns, but it places them at {:?}",
        placement.columns
    );
}

/// WHY: the third tile shows that the same two ranges express every axis-aligned transform at once. It must mirror
/// the image in x and make it upright in y (both ranges descend), and it must stretch it in both directions (neither
/// pitch is one data unit) by an amount that a reader can see in the tick labels. The two pitches must differ from
/// each other, because equal pitches would not show that the two ranges are set independently of each other.
#[test]
fn third_tile_flips_and_stretches_in_both_directions() {
    let figure = figure();
    let placement = three_images(&figure)[2].placement;
    let columns = placement
        .columns
        .expect("the third tile places its columns explicitly");
    let rows = placement
        .rows
        .expect("the third tile places its rows explicitly");
    assert!(
        columns.first > columns.last,
        "the column range of the third tile must descend to mirror the image, but it runs from {} to {}",
        columns.first,
        columns.last
    );
    assert!(
        rows.first > rows.last,
        "the row range of the third tile must descend so that row 0 is at the top, but it runs from {} to {}",
        rows.first,
        rows.last
    );
    for (name, range) in [("column", columns), ("row", rows)] {
        assert!(
            (pitch(range).abs() - 1.0).abs() >= VISIBLE_STRETCH,
            "the {name} range of the third tile does not visibly stretch the image: the size of its pitch is {}, \
             which is within {VISIBLE_STRETCH} of the default pitch of 1",
            pitch(range).abs()
        );
    }
    assert!(
        (pitch(columns).abs() - pitch(rows).abs()).abs() >= VISIBLE_STRETCH,
        "the third tile must stretch the two directions by visibly different amounts, to show that the ranges are \
         independent, but the sizes of the pitches are {} along the columns and {} along the rows",
        pitch(columns).abs(),
        pitch(rows).abs()
    );
}

/// WHY: the three tiles show the same photograph, so a reader can tell which placement produced which tile only from
/// the titles of the axes. Every tile must therefore carry a title, and no two titles may be the same.
#[test]
fn every_tile_has_a_title_of_its_own() {
    let figure = figure();
    let titles: Vec<String> = axes_in_reading_order(&figure)
        .iter()
        .map(|a| {
            a.title
                .as_ref()
                .map_or_else(String::new, |t| t.content.trim().to_owned())
        })
        .collect();
    for (col, title) in titles.iter().enumerate() {
        assert!(
            !title.is_empty(),
            "tile {col} has no title, so a reader cannot tell which placement it shows"
        );
        assert_eq!(
            titles.iter().filter(|other| *other == title).count(),
            1,
            "the title {title:?} of tile {col} is shared with another tile, so it does not tell the placements apart"
        );
    }
}

/// WHY: the credit on the page names one specific photograph, so the entry must draw that photograph and nothing
/// else. Comparing the pixel array with a decoding of the asset made by the test itself catches a replaced asset
/// path, a conversion that reorders or rescales the bytes, and an entry that reverses the rows before placing them.
/// Row 0 of the array must be the top row of the file, because the whole entry rests on that convention. Each grey
/// level must appear unchanged in the red, green and blue components, which is how a true-colour image shows grey.
#[test]
fn the_entry_draws_the_asset_with_its_top_row_first() {
    let figure = figure();
    let array = pixels(&figure, three_images(&figure)[0]);
    let bytes = array.as_u8().expect("the photograph is stored as bytes");
    let asset = decoded_asset();
    let expected: Vec<u8> = asset
        .as_raw()
        .iter()
        .flat_map(|&grey| [grey, grey, grey])
        .collect();
    assert_eq!(
        array.shape,
        vec![SIDE, SIDE, 3],
        "the pixel array is not the {SIDE} by {SIDE} photograph with three components per pixel"
    );
    assert!(
        bytes == expected.as_slice(),
        "the pixels of the entry are not the pixels of {} in the row order of the file",
        asset_path().display()
    );
}

/// WHY: the photograph was chosen in black and white, and was downsampled to a small square, so that the repository
/// carries no heavy binary file. These properties belong to the asset rather than to the entry, and a well-meant
/// replacement with a larger or coloured version of the photograph would break them without breaking anything else.
/// A photograph also has many grey levels, so an image with few levels would mean that the asset had been replaced
/// by a placeholder.
#[test]
fn the_asset_is_a_small_square_greyscale_photograph() {
    let path = asset_path();
    let size = fs::metadata(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        .len();
    assert!(
        size < 100 * 1024,
        "{} is {size} bytes, which is too heavy for an asset kept in the repository",
        path.display()
    );

    let decoded = image::open(&path).expect("the asset decodes");
    assert_eq!(
        decoded.color(),
        image::ColorType::L8,
        "the asset must be stored as 8-bit greyscale, which is a third of the size of the same image in colour"
    );
    assert_eq!(
        (decoded.width() as usize, decoded.height() as usize),
        (SIDE, SIDE),
        "the asset must be a square of {SIDE} pixels, which is the size that the placement tests assume"
    );

    let grey = decoded.into_luma8();
    let mut seen = [false; 256];
    for &level in grey.as_raw() {
        seen[usize::from(level)] = true;
    }
    let levels = seen.iter().filter(|&&s| s).count();
    assert!(
        levels > 64,
        "the asset has only {levels} grey levels, so it is not a photograph"
    );
}

/// WHY: the licence of the photograph (CC BY-SA 4.0) requires that every publication of it identifies the author,
/// links to the source, names the licence with a link to its text and says that the work was modified. The practice
/// that Creative Commons recommends (title, author, source and licence) adds the title of the work. The page of the
/// entry is a publication of the photograph, so it must carry all of these. The two links are looked for among the
/// link targets outside fenced code, because an address inside the source listing is not a link that a reader can
/// follow.
#[test]
fn the_entry_page_credits_the_photograph() {
    let page = entry_markdown(&entry());
    let caption = common::without_fences(&page);
    for required in [AUTHOR, WORK_TITLE, LICENCE_NAME] {
        assert!(
            caption.contains(required),
            "the page does not mention {required:?} outside its source listing"
        );
    }
    for modification in MODIFICATIONS {
        assert!(
            mentions(&caption, modification),
            "the page does not say that the photograph was {modification}, which the licence requires of an \
             adapted work"
        );
    }
    let targets = link_targets(&page);
    assert!(
        targets.iter().any(|t| t == SOURCE_URL),
        "the page has no link to the photograph on Wikimedia Commons; its link targets are {targets:?}"
    );
    assert!(
        targets.iter().any(|t| is_licence_link(t)),
        "the page names the licence but has no link to its text at {LICENCE_URL}; its link targets are {targets:?}"
    );
}

/// WHY: the index shows a thumbnail of the figure, which is also a publication of the photograph, so the card of the
/// entry must name the author and the licence beside it. The card shows the same description as the page of the
/// entry, so this requirement costs the entry nothing; the test exists so that a later change that shortens the text
/// of the cards cannot silently remove the credit from beside the thumbnail.
#[test]
fn the_index_card_credits_the_photograph() {
    let index = index_markdown(&[entry()]);
    for required in [AUTHOR, LICENCE_NAME] {
        assert!(
            index.contains(required),
            "the index card of the entry does not mention {required:?}"
        );
    }
}

/// WHY: the generated pages are not part of the repository, so a person who finds the file in the repository needs
/// the same attribution beside it. The notice must identify the file, its author, its source and its licence by
/// address, and must state the modifications, because CC BY-SA 4.0 requires an adapted work to say that it was
/// adapted and to remain under the same licence.
#[test]
fn a_licence_notice_accompanies_the_asset() {
    let path = assets_dir().join("LICENSE.md");
    let notice = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read the licence notice {}: {e}", path.display()));
    for required in [
        "zebra_eye.png",
        AUTHOR,
        WORK_TITLE,
        SOURCE_URL,
        LICENCE_NAME,
        LICENCE_URL,
    ] {
        assert!(
            notice.contains(required),
            "{} does not mention {required:?}",
            path.display()
        );
    }
    for modification in MODIFICATIONS {
        assert!(
            mentions(&notice, modification),
            "{} does not record that the photograph was modified: it lacks {modification:?}",
            path.display()
        );
    }
}

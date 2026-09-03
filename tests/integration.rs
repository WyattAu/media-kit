//! End-to-end tests for the enabled formats.

use media_kit::encode::{encode, OutFormat};
use media_kit::meta::{dimensions, enforce_limits, Limits};
use media_kit::pipeline::Pipeline;
use media_kit::resize::{Fit, Filter};
use media_kit::sniff::{sniff, Format};
use media_kit::testutil::{noise_image, tiny_jpeg, tiny_png};
use media_kit::variants::{Variant, VariantSet};

#[test]
fn roundtrip_jpeg() {
    let src = noise_image(24, 18);
    let bytes = encode(&src, &OutFormat::Jpeg(85)).unwrap();
    assert_eq!(sniff(&bytes), Some(Format::Jpeg));
    let back = image::load_from_memory(&bytes).unwrap();
    assert_eq!((back.width(), back.height()), (24, 18));
}

#[test]
fn roundtrip_png() {
    let src = noise_image(24, 18);
    let bytes = encode(&src, &OutFormat::Png).unwrap();
    assert_eq!(sniff(&bytes), Some(Format::Png));
    let back = image::load_from_memory(&bytes).unwrap();
    assert_eq!((back.width(), back.height()), (24, 18));
}

#[test]
fn roundtrip_gif() {
    let src = noise_image(24, 18);
    let bytes = encode(&src, &OutFormat::Gif).unwrap();
    assert_eq!(sniff(&bytes), Some(Format::Gif));
    let back = image::load_from_memory(&bytes).unwrap();
    assert_eq!((back.width(), back.height()), (24, 18));
}

#[test]
fn roundtrip_webp() {
    let src = noise_image(24, 18);
    let bytes = encode(&src, &OutFormat::WebP(None)).unwrap();
    assert_eq!(sniff(&bytes), Some(Format::WebP));
    let back = image::load_from_memory(&bytes).unwrap();
    assert_eq!((back.width(), back.height()), (24, 18));
}

#[test]
fn resize_dimensions_match_fit() {
    let src = noise_image(1000, 500);
    for (fit, want) in [
        (Fit::Width(200), (200, 100)),
        (Fit::Height(250), (500, 250)),
        (Fit::MaxSide(100), (100, 50)),
        (Fit::Exact(40, 60), (40, 60)),
    ] {
        let out = media_kit::resize::resize(&src, &fit, Filter::Lanczos3);
        assert_eq!(
            (out.width(), out.height()),
            want,
            "fit {fit:?} produced wrong dims"
        );
    }
}

#[test]
fn cover_crop_is_exactly_square() {
    let src = noise_image(1024, 300);
    let out = media_kit::resize::resize(&src, &Fit::Cover(96, 96), Filter::Triangle);
    assert_eq!(out.width(), out.height());
    assert_eq!(out.width(), 96);
}

#[test]
fn sniff_rejects_garbage() {
    let mut rng_state = 0x12345678u32;
    for _ in 0..256 {
        rng_state = rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
        let bytes = rng_state.to_be_bytes().repeat(4);
        if sniff(&bytes).is_some() {
            // Only possible if we accidentally generated a magic prefix;
            // the multiplier constants make that essentially impossible.
            panic!("sniff accepted garbage: {bytes:?}");
        }
    }
    assert_eq!(sniff(b""), None);
    assert_eq!(sniff(&[0u8; 32]), None);
}

#[test]
fn enforce_limits_rejects_11mib() {
    let limits = Limits::new().max_bytes(10 * 1024 * 1024);
    // 11 MiB buffer of junk — must fail on size before even sniffing.
    let big = vec![0u8; 11 * 1024 * 1024];
    let err = enforce_limits(&big, &limits).unwrap_err();
    match err {
        media_kit::MediaError::TooLarge { limit_bytes, got } => {
            assert_eq!(limit_bytes, 10 * 1024 * 1024);
            assert_eq!(got, 11 * 1024 * 1024);
        }
        other => panic!("wrong error: {other}"),
    }
}

#[test]
fn pipeline_thumb_to_webp_end_to_end() {
    let jpg = tiny_jpeg();
    let out = Pipeline::new(OutFormat::WebP(None))
        .resize(Fit::MaxSide(6), Filter::Lanczos3)
        .strip_exif()
        .run(&jpg)
        .unwrap();
    assert_eq!(sniff(&out), Some(Format::WebP));
    let dims = dimensions(&out).unwrap();
    assert!(dims.0 <= 6 && dims.1 <= 6);
}

#[test]
fn variants_web_standard_three_outputs_expected_dims() {
    let set = VariantSet::web_standard();
    let img = noise_image(2048, 1536);
    let outs = set.generate(&img).unwrap();
    assert_eq!(outs.len(), 3);

    let expect = [("thumb", (200u32, 150u32)), ("medium", (800, 600)), ("large", (1920, 1440))];
    for ((name, fmt, bytes), (want_name, want_dims)) in outs.iter().zip(expect) {
        assert_eq!(name, want_name);
        assert_eq!(*fmt, OutFormat::WebP(None));
        let d = dimensions(bytes).unwrap();
        assert_eq!(d, want_dims, "{name} dims mismatch");
    }
}

#[test]
fn custom_avatar_variant_is_square() {
    let set = VariantSet::new().with(Variant::new(
        "avatar",
        Fit::Cover(128, 128),
        OutFormat::Jpeg(90),
    ));
    let outs = set.generate(&noise_image(500, 200)).unwrap();
    let img = image::load_from_memory(&outs[0].2).unwrap();
    assert_eq!((img.width(), img.height()), (128, 128));
}

#[test]
fn dimensions_header_only_works_for_all_enabled() {
    assert_eq!(dimensions(&tiny_jpeg()), Some((8, 8)));
    assert_eq!(dimensions(&tiny_png(9, 5)), Some((9, 5)));
}

#[test]
fn jpeg_quality_scales_size() {
    let src = noise_image(128, 128);
    let hi = encode(&src, &OutFormat::Jpeg(95)).unwrap().len();
    let lo = encode(&src, &OutFormat::Jpeg(20)).unwrap().len();
    assert!(hi > lo);
}

#[test]
fn pipeline_rejects_bad_input() {
    assert!(Pipeline::new(OutFormat::Png).run(b"junk").is_err());
    let jpg = tiny_jpeg();
    assert!(
        Pipeline::new(OutFormat::Png)
            .limits(Limits::new().max_bytes(2))
            .run(&jpg)
            .is_err()
    );
}

#[cfg(feature = "serde")]
#[test]
fn serde_roundtrip_configs() {
    let limits = Limits::new().max_bytes(1024).max_width(100).max_height(200);
    let json = serde_json::to_string(&limits).unwrap();
    assert_eq!(serde_json::from_str::<Limits>(&json).unwrap(), limits);

    let set = VariantSet::web_standard();
    let json = serde_json::to_string(&set).unwrap();
    assert_eq!(serde_json::from_str::<VariantSet>(&json).unwrap(), set);

    let fmt = OutFormat::Jpeg(85);
    let json = serde_json::to_string(&fmt).unwrap();
    assert_eq!(serde_json::from_str::<OutFormat>(&json).unwrap(), fmt);
}

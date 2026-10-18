//! Independent geometry calculation from pre-step XML and embedded pixels.
use crate::contract::FitMode;
use crate::{attr, change, fail, hp};
use hae_diag::{Limits, Result};
use hae_package::{PackageInfo, PackageState, NS_HC};
use hae_xml::XmlDoc;
fn one(d: &XmlDoc, p: usize, name: &str, ns_core: bool) -> Result<usize> {
    let mut found = d.el(p).children.iter().copied().filter(|&i| {
        let e = d.el(i);
        e.local == name
            && (if ns_core {
                e.ns.as_deref() == Some(NS_HC)
            } else {
                hp(e, name)
            })
    });
    let first = found
        .next()
        .ok_or_else(|| fail(format!("missing {name}")))?;
    if found.next().is_some() {
        return Err(fail(format!("ambiguous {name}")));
    }
    Ok(first)
}
fn integer(d: &XmlDoc, i: usize, key: &str) -> Result<u64> {
    attr(d, i, key)?
        .value
        .parse::<u32>()
        .map(u64::from)
        .map_err(|_| fail("invalid geometry integer"))
}
fn rounded(n: u64, den: u64) -> Result<u64> {
    let value = (n + den / 2) / den;
    if value == 0 || value > u64::from(u32::MAX) {
        return Err(fail("geometry out of range"));
    }
    Ok(value)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    state: &mut PackageState,
    info: &PackageInfo,
    entry: &str,
    d: &XmlDoc,
    p: usize,
    mode: FitMode,
    limits: &Limits,
) -> Result<bool> {
    if !hp(d.el(p), "pic") {
        return Err(fail("image.fit target must be picture"));
    }
    let org = one(d, p, "orgSz", false)?;
    let cur = one(d, p, "curSz", false)?;
    let size = one(d, p, "sz", false)?;
    let ow = integer(d, org, "width")?;
    let oh = integer(d, org, "height")?;
    if ow == 0 || oh == 0 {
        return Err(fail("empty original size"));
    }
    let actual_w = integer(d, cur, "width")?;
    let actual_h = integer(d, cur, "height")?;
    let oldw = if actual_w == 0 { ow } else { actual_w };
    let oldh = if actual_h == 0 { oh } else { actual_h };
    let sizew = integer(d, size, "width")?;
    let sizeh = integer(d, size, "height")?;
    let fw = if sizew == 0 { oldw } else { sizew };
    let fh = if sizeh == 0 { oldh } else { sizeh };
    let im = one(d, p, "img", true)?;
    let media = info
        .item_by_id(&attr(d, im, "binaryItemIDRef")?.value)
        .ok_or_else(|| fail("missing embedded image item"))?;
    let bytes = state
        .get(&media.entry)
        .ok_or_else(|| fail("missing image payload"))?;
    let (_, pixels) = crate::image::validated_media_type(bytes, limits)?;
    let iw = u64::from(pixels.0)
        .checked_mul(75)
        .filter(|v| *v <= u64::from(u32::MAX))
        .ok_or_else(|| fail("image width overflow"))?;
    let ih = u64::from(pixels.1)
        .checked_mul(75)
        .filter(|v| *v <= u64::from(u32::MAX))
        .ok_or_else(|| fail("image height overflow"))?;
    let mut width = fw;
    let mut height = fh;
    let mut right = iw;
    let mut bottom = ih;
    match mode {
        FitMode::Contain => {
            if iw * fh > ih * fw {
                height = rounded(fw * ih, iw)?.min(fh);
            } else {
                width = rounded(fh * iw, ih)?.min(fw);
            }
        }
        FitMode::Cover => {
            if iw * fh > ih * fw {
                right = rounded(ih * fw, fh)?.min(iw);
            } else {
                bottom = rounded(iw * fh, fw)?.min(ih);
            }
        }
        FitMode::Stretch => {}
    }
    let left = (iw - right) / 2;
    let top = (ih - bottom) / 2;
    let render = one(d, p, "renderingInfo", false)?;
    let mut cuts = Vec::new();
    for name in ["transMatrix", "scaMatrix", "rotMatrix"] {
        let matrix = one(d, render, name, true)?;
        for k in 1..=6 {
            let key = format!("e{k}");
            let a = attr(d, matrix, &key)?;
            let old = a
                .value
                .parse::<f64>()
                .map_err(|_| fail("invalid render matrix"))?;
            if !old.is_finite() {
                return Err(fail("nonfinite render matrix"));
            }
            if name == "scaMatrix" {
                let (num, den) = if k <= 3 {
                    (width, oldw)
                } else {
                    (height, oldh)
                };
                if num != den {
                    let v = old * (num as f64) / (den as f64);
                    if !v.is_finite() {
                        return Err(fail("render matrix overflow"));
                    }
                    cuts.push((a.value_span.clone(), format!("{v:.6}").into_bytes()));
                }
            }
        }
    }
    let rect = one(d, p, "imgRect", false)?;
    let expected = [
        (cur, vec![("width", width), ("height", height)]),
        (size, vec![("width", width), ("height", height)]),
        (
            one(d, p, "imgDim", false)?,
            vec![("dimwidth", iw), ("dimheight", ih)],
        ),
        (
            one(d, p, "imgClip", false)?,
            vec![
                ("left", left),
                ("top", top),
                ("right", left + right),
                ("bottom", top + bottom),
            ],
        ),
        (one(d, rect, "pt0", true)?, vec![("x", 0), ("y", 0)]),
        (one(d, rect, "pt1", true)?, vec![("x", ow), ("y", 0)]),
        (one(d, rect, "pt2", true)?, vec![("x", ow), ("y", oh)]),
        (one(d, rect, "pt3", true)?, vec![("x", 0), ("y", oh)]),
    ];
    for (el, attrs) in expected {
        for (key, value) in attrs {
            let source = attr(d, el, key)?;
            let wanted = value.to_string();
            if source.value != wanted {
                cuts.push((source.value_span.clone(), wanted.into_bytes()));
            }
        }
    }
    let changed = !cuts.is_empty();
    change(state, entry, d, cuts)?;
    Ok(changed)
}

//! Source-space clipping and display-space fitting; media is never changed.
use crate::{Draft, ImageBytes, Intent};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{is_hp, Kind};
use hae_package::NS_HC;
use hae_xml::XmlDoc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FitMode {
    Contain,
    Cover,
    Stretch,
}
fn bad() -> EngineError {
    EngineError::new(
        Code::Unsupported,
        Stage::Validate,
        "image.fit needs complete finite image geometry and rendering matrices",
    )
    .with_reason("image_geometry")
}
fn child(doc: &XmlDoc, p: usize, name: &str, core: bool) -> Result<usize> {
    let v: Vec<_> = doc
        .el(p)
        .children
        .iter()
        .copied()
        .filter(|&c| {
            let e = doc.el(c);
            e.local == name
                && if core {
                    e.ns.as_deref() == Some(NS_HC)
                } else {
                    is_hp(e.ns.as_deref())
                }
        })
        .collect();
    if v.len() != 1 {
        return Err(bad());
    }
    Ok(v[0])
}
fn num(doc: &XmlDoc, i: usize, key: &str) -> Result<u32> {
    doc.el(i)
        .attr_value(key)
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(bad)
}
fn ratio(a: u32, b: u32, c: u32) -> Result<u32> {
    u32::try_from((u64::from(a) * u64::from(b) + u64::from(c) / 2) / u64::from(c))
        .ok()
        .filter(|&n| n > 0)
        .ok_or_else(bad)
}
pub(crate) fn fit(d: &mut Draft, target: &str, mode: FitMode) -> Result<()> {
    let n = d.node(target, Some(Kind::Picture))?;
    let doc = d.doc.doc_of(&n).clone();
    let pic = n.el;
    let org = child(&doc, pic, "orgSz", false)?;
    let cur = child(&doc, pic, "curSz", false)?;
    let sz = child(&doc, pic, "sz", false)?;
    let (ow, oh) = (num(&doc, org, "width")?, num(&doc, org, "height")?);
    if ow == 0 || oh == 0 {
        return Err(bad());
    }
    let (cw, ch) = (num(&doc, cur, "width")?, num(&doc, cur, "height")?);
    let (fw, fh) = (num(&doc, sz, "width")?, num(&doc, sz, "height")?);
    let (fw, fh) = (
        if fw == 0 {
            if cw == 0 {
                ow
            } else {
                cw
            }
        } else {
            fw
        },
        if fh == 0 {
            if ch == 0 {
                oh
            } else {
                ch
            }
        } else {
            fh
        },
    );
    let img = child(&doc, pic, "img", true)?;
    let id = doc.el(img).attr_value("binaryItemIDRef").ok_or_else(bad)?;
    let media = d.doc.info.item_by_id(id).ok_or_else(bad)?;
    let payload = d.doc.payload(&media.entry).ok_or_else(bad)?;
    let image = ImageBytes::from_bytes(payload.to_vec(), &d.doc.limits)?;
    let (pw, ph) = image.dimensions();
    let (iw, ih) = (
        pw.checked_mul(75).ok_or_else(bad)?,
        ph.checked_mul(75).ok_or_else(bad)?,
    );
    let wide = u64::from(iw) * u64::from(fh) > u64::from(ih) * u64::from(fw);
    let (mut w, mut h) = (fw, fh);
    let (mut cropw, mut croph) = (iw, ih);
    match mode {
        FitMode::Contain => {
            if wide {
                h = ratio(fw, ih, iw)?.min(fh);
            } else {
                w = ratio(fh, iw, ih)?.min(fw);
            }
        }
        FitMode::Cover => {
            if wide {
                cropw = ratio(ih, fw, fh)?.min(iw);
            } else {
                croph = ratio(iw, fh, fw)?.min(ih);
            }
        }
        FitMode::Stretch => {}
    }
    let (left, top) = ((iw - cropw) / 2, (ih - croph) / 2);
    let ri = child(&doc, pic, "renderingInfo", false)?;
    let mut cuts = Vec::new();
    // Keep all matrix elements and the original transMatrix/rotMatrix bytes.
    // Scale the scaMatrix affine rows, including off-diagonal/translation terms.
    let sca = child(&doc, ri, "scaMatrix", true)?;
    for name in ["transMatrix", "scaMatrix", "rotMatrix"] {
        let i = child(&doc, ri, name, true)?;
        for k in ["e1", "e2", "e3", "e4", "e5", "e6"] {
            let a = doc.el(i).attr(k).ok_or_else(bad)?;
            let value = a
                .value
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(bad)?;
            if i == sca {
                let (new, old) = if ["e1", "e2", "e3"].contains(&k) {
                    (w, if cw == 0 { ow } else { cw })
                } else {
                    (h, if ch == 0 { oh } else { ch })
                };
                if new != old {
                    let scaled = value * f64::from(new) / f64::from(old);
                    if !scaled.is_finite() {
                        return Err(bad());
                    }
                    cuts.push((a.value_span.clone(), format!("{scaled:.6}").into_bytes()));
                }
            }
        }
    }
    let rect = child(&doc, pic, "imgRect", false)?;
    let dim = child(&doc, pic, "imgDim", false)?;
    let clip = child(&doc, pic, "imgClip", false)?;
    for (el, attrs) in [
        (cur, vec![("width", w), ("height", h)]),
        (sz, vec![("width", w), ("height", h)]),
        (dim, vec![("dimwidth", iw), ("dimheight", ih)]),
        (
            clip,
            vec![
                ("left", left),
                ("top", top),
                ("right", left + cropw),
                ("bottom", top + croph),
            ],
        ),
        (child(&doc, rect, "pt0", true)?, vec![("x", 0), ("y", 0)]),
        (child(&doc, rect, "pt1", true)?, vec![("x", ow), ("y", 0)]),
        (child(&doc, rect, "pt2", true)?, vec![("x", ow), ("y", oh)]),
        (child(&doc, rect, "pt3", true)?, vec![("x", 0), ("y", oh)]),
    ] {
        for (key, v) in attrs {
            let a = doc.el(el).attr(key).ok_or_else(bad)?;
            if a.value != v.to_string() {
                cuts.push((a.value_span.clone(), v.to_string().into_bytes()));
            }
        }
    }
    if cuts.is_empty() {
        return Ok(());
    }
    let path = doc.path(pic);
    d.splice_entry(&n.entry, cuts)?;
    let containing_para = d
        .doc
        .doc_of(d.doc.require(target, None)?)
        .ancestors(d.doc.require(target, None)?.el)
        .find_map(|a| {
            d.doc
                .nodes
                .iter()
                .find(|p| p.entry == n.entry && p.el == a && p.kind == Kind::Paragraph)
        })
        .cloned();
    if let Some(p) = containing_para {
        d.anchor(&p);
    }
    d.changed.push(target.into());
    d.intents.push(Intent::ImageFit {
        entry: n.entry,
        path,
        mode,
    });
    Ok(())
}

"""Package/reference regressions from the first native Windows open failure.

These checks catch omissions in the generator; they do not prove native opening.
"""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "evaluate_mcp", Path(__file__).resolve().parents[1] / "evaluate_mcp.py")
evaluation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evaluation)
HH = "{http://www.hancom.co.kr/hwpml/2011/head}"
HP = "{http://www.hancom.co.kr/hwpml/2011/paragraph}"
HC = "{http://www.hancom.co.kr/hwpml/2011/core}"
OPF = "{http://www.idpf.org/2007/opf/}"


class FixtureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "fixture.hwpx"
        evaluation.build_hwpx(self.path, evaluation.synth_png(1, 1, False))
        self.z = zipfile.ZipFile(self.path)
        self.addCleanup(self.z.close)

    def test_package_infrastructure_and_registered_payloads(self):
        self.assertTrue({"version.xml", "settings.xml", "META-INF/container.rdf",
                         "META-INF/manifest.xml", "Preview/PrvText.txt"} <= set(self.z.namelist()))
        first = self.z.infolist()[0]
        self.assertEqual((first.filename, first.compress_type), ("mimetype", 0))
        self.assertEqual(self.z.read("mimetype"), b"application/hwp+zip")
        hpf = ET.fromstring(self.z.read("Contents/content.hpf"))
        manifest = {x.attrib["id"]: x.attrib for x in hpf.findall(OPF + "manifest/" + OPF + "item")}
        for item in manifest.values():
            self.assertIn(item["href"], self.z.namelist())
        self.assertEqual(manifest["image1"]["media-type"], "image/png")
        self.assertEqual(manifest["settings"]["href"], "settings.xml")
        for item in hpf.findall(OPF + "spine/" + OPF + "itemref"):
            self.assertIn(item.attrib["idref"], manifest)

    def test_header_has_complete_reference_tables_and_resolves_section(self):
        h = ET.fromstring(self.z.read("Contents/header.xml"))
        refs = h.find(HH + "refList")
        self.assertIsNotNone(h.find(HH + "beginNum"))
        for name in ["fontfaces", "borderFills", "charProperties", "paraProperties",
                     "tabProperties", "numberings", "styles"]:
            self.assertIsNotNone(refs.find(HH + name), name)
        for el in h.iter():
            for attr in ["itemCnt", "fontCnt"]:
                if attr in el.attrib:
                    self.assertEqual(int(el.attrib[attr]), len(el), el.tag)
        maps = {attr: {x.attrib["id"] for x in refs.find(HH + name)} for attr, name in [
            ("charPrIDRef", "charProperties"), ("paraPrIDRef", "paraProperties"),
            ("styleIDRef", "styles"), ("borderFillIDRef", "borderFills")]}
        section = ET.fromstring(self.z.read("Contents/section0.xml"))
        for el in list(h.iter()) + list(section.iter()):
            for attr, ids in maps.items():
                if attr in el.attrib:
                    # Native paraHead uses UINT_MAX for inherited character style.
                    if el.tag == HH + "paraHead" and attr == "charPrIDRef" and el.attrib[attr] == "4294967295":
                        continue
                    self.assertIn(el.attrib[attr], ids, (el.tag, attr))
        for p in section.iter(HP + "p"):
            self.assertTrue({"id", "styleIDRef", "pageBreak", "columnBreak", "merged"} <= p.attrib.keys())
        fonts = {x.attrib["lang"].lower(): {f.attrib["id"] for f in x}
                 for x in refs.find(HH + "fontfaces")}
        self.assertEqual(set(fonts), {"hangul", "latin", "hanja", "japanese", "other", "symbol", "user"})
        for font_ref in h.iter(HH + "fontRef"):
            for lang, value in font_ref.attrib.items():
                self.assertIn(value, fonts[lang])

    def test_section_and_shapes_include_native_serialization_fields(self):
        s = ET.fromstring(self.z.read("Contents/section0.xml"))
        sec = s.find(HP + "p/" + HP + "run/" + HP + "secPr")
        for name in ["pagePr", "footNotePr", "endNotePr", "pageBorderFill"]:
            self.assertIsNotNone(sec.find(HP + name), name)
        table = next(s.iter(HP + "tbl"))
        self.assertTrue({"id", "zOrder", "numberingType", "textWrap", "pageBreak", "repeatHeader"} <= table.attrib.keys())
        for cell in table.iter(HP + "tc"):
            for name in ["subList", "cellAddr", "cellSpan", "cellSz", "cellMargin"]:
                self.assertIsNotNone(cell.find(HP + name), name)
        pic = next(s.iter(HP + "pic"))
        for name in ["offset", "orgSz", "curSz", "flip", "rotationInfo", "renderingInfo",
                     "imgRect", "imgClip", "imgDim", "effects", "sz", "pos", "outMargin"]:
            self.assertIsNotNone(pic.find(HP + name), name)
        self.assertEqual(pic.find(HC + "img").attrib["binaryItemIDRef"], "image1")


if __name__ == "__main__":
    unittest.main()

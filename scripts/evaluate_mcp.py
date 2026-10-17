#!/usr/bin/env python3
"""Linux 공개/합성 시험 자료 대표 시나리오 end-to-end 평가 (설계서 §1·§13).

실제 stdio MCP 서버(`hae-mcp`)를 띄워 4가지 편집(셀 텍스트·문단 정렬·그림 바이트)을
포함한 대표 시나리오를 실행하고, 각 단계의 응답 로그·파일 해시·판정을 출력 디렉터리에
남긴다. Python 3 표준 라이브러리만 쓴다.

평가 대상(설계서 §1 대표 시나리오, §13 시험표):
  open → snapshot → preview(before) → query(명시 대상, 유일) → dryRun batch
  → 원본 무편집 내보내기 바이트 동일 → batch 적용(revision+1 정확히 한 번)
  → diff → preview(after) → undo(원본 파일 바이트 exact) → 교체 원본 파일 변조
  → redo(편집 payload 동일) → project 저장 → 편집 export → close → project 재열기(ID·revision 유지)

경계:
  * 개인 문서를 쓰지 않는다. 시험 문서는 공개 MIT ref_empty.hwpx의 완전한 기본 구조에 합성 본문을 넣는다.
    교체 이미지는 tests/fixtures/editing/inputs/ok.png 를 쓴다.
  * 한컴 조판 호환을 주장하지 않는다. summary.nativeComparison 은 항상 `not_checked`다.
  * 기존 근거를 덮어쓰지 않는다. 출력 디렉터리는 비어 있어야 하며 매 실행 새 경로를 쓴다.
  * 실행이 실패해도 summary.json 을 쓰고 0이 아닌 코드로 끝난다. 엔진 소스는 바꾸지 않는다.

사용:
    python3 scripts/evaluate_mcp.py --output-dir spikes/work/stage4-linux/<run>
    python3 scripts/evaluate_mcp.py --help
"""

import argparse
import base64
import hashlib
import io
import json
import os
import re
import select
import shutil
import struct
import subprocess
import sys
import threading
import time
import zipfile
import zlib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>'
MAX_INLINE_BYTES = 4 * 1024 * 1024  # 인라인 PNG/SVG 추출 상한

# Public MIT corpus seed: tests/fixtures/corpus/manifest.json records provenance/hash.
# Keep complete native reference tables and section defaults; never read user_fixtures here.
PUBLIC_SEED = REPO_ROOT / "tests/fixtures/corpus/ref_empty.hwpx"
P_ATTRS = ('id="{id}" paraPrIDRef="0" styleIDRef="0" '
           'pageBreak="0" columnBreak="0" merged="0"')
BORDER_FILL = (
    '<hh:borderFill id="3" threeD="0" shadow="0" centerLine="NONE" breakCellSeparateLine="0">'
    '<hh:slash type="NONE" Crooked="0" isCounter="0"/><hh:backSlash type="NONE" Crooked="0" isCounter="0"/>'
    '<hh:leftBorder type="SOLID" width="0.12 mm" color="#000000"/>'
    '<hh:rightBorder type="SOLID" width="0.12 mm" color="#000000"/>'
    '<hh:topBorder type="SOLID" width="0.12 mm" color="#000000"/>'
    '<hh:bottomBorder type="SOLID" width="0.12 mm" color="#000000"/>'
    '<hh:diagonal type="SOLID" width="0.1 mm" color="#000000"/></hh:borderFill>'
)


def table_cell_xml(col: int, text: str) -> str:
    return (
        '<hp:tc name="" header="0" hasMargin="0" protect="0" editable="0" dirty="0" borderFillIDRef="3">'
        '<hp:subList id="" textDirection="HORIZONTAL" lineWrap="BREAK" vertAlign="CENTER" '
        'linkListIDRef="0" linkListNextIDRef="0" textWidth="0" textHeight="0" hasTextRef="0" hasNumRef="0">'
        f'<hp:p {P_ATTRS.format(id=10 + col)}><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p>'
        '</hp:subList>'
        f'<hp:cellAddr colAddr="{col}" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/>'
        '<hp:cellSz width="21000" height="2400"/>'
        '<hp:cellMargin left="141" right="141" top="141" bottom="141"/></hp:tc>'
    )


def table_xml() -> str:
    return (
        '<hp:tbl id="1001" zOrder="0" numberingType="TABLE" textWrap="TOP_AND_BOTTOM" '
        'textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" pageBreak="CELL" repeatHeader="1" '
        'rowCnt="1" colCnt="2" cellSpacing="0" borderFillIDRef="3" noAdjust="0">'
        '<hp:sz width="42000" widthRelTo="ABSOLUTE" height="2400" heightRelTo="ABSOLUTE" protect="0"/>'
        '<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" '
        'vertRelTo="PARA" horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>'
        '<hp:outMargin left="283" right="283" top="283" bottom="283"/>'
        '<hp:inMargin left="141" right="141" top="141" bottom="141"/>'
        f'<hp:tr>{table_cell_xml(0, "셀가")}{table_cell_xml(1, "셀나")}</hp:tr></hp:tbl>'
    )


def picture_xml() -> str:
    # Fields/order observed in public bodojaryo-reference.hwpx and native saved files.
    # Identity transforms, full clip, empty effects: a plain inline picture.
    return (
        '<hp:pic id="1002" zOrder="1" numberingType="PICTURE" textWrap="SQUARE" '
        'textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" href="" groupLevel="0" instid="1002" reverse="0">'
        '<hp:offset x="0" y="0"/><hp:orgSz width="14400" height="7200"/>'
        '<hp:curSz width="0" height="0"/><hp:flip horizontal="0" vertical="0"/>'
        '<hp:rotationInfo angle="0" centerX="7200" centerY="3600" rotateimage="1"/>'
        '<hp:renderingInfo>'
        '<hc:transMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>'
        '<hc:scaMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>'
        '<hc:rotMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/></hp:renderingInfo>'
        '<hc:img binaryItemIDRef="image1" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/>'
        '<hp:imgRect><hc:pt0 x="0" y="0"/><hc:pt1 x="14400" y="0"/>'
        '<hc:pt2 x="14400" y="7200"/><hc:pt3 x="0" y="7200"/></hp:imgRect>'
        '<hp:imgClip left="0" right="75" top="0" bottom="75"/>'
        '<hp:inMargin left="0" right="0" top="0" bottom="0"/>'
        '<hp:imgDim dimwidth="75" dimheight="75"/><hp:effects/>'
        '<hp:sz width="14400" widthRelTo="ABSOLUTE" height="7200" heightRelTo="ABSOLUTE" protect="0"/>'
        '<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" '
        'vertRelTo="PARA" horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>'
        '<hp:outMargin left="0" right="0" top="0" bottom="0"/></hp:pic>'
    )


def section_xml(*, table: bool = True, picture: bool = True) -> str:
    """Complete public-seed section defaults plus synthetic, explicitly sized content."""
    with zipfile.ZipFile(PUBLIC_SEED) as z:
        section = z.read("Contents/section0.xml").decode("utf-8")
    # Retain the native first paragraph/secPr/colPr and all namespace declarations.
    # Cached geometry belongs to the old empty text and is deliberately removed.
    section = re.sub(r"<hp:linesegarray>.*?</hp:linesegarray>", "", section, flags=re.S)
    section = section.replace('<hp:t/>', '<hp:t>평가 문단 하나</hp:t>', 1)
    extras = ""
    for pid, content in [(2, table_xml() if table else ""), (3, picture_xml() if picture else "")]:
        if content:
            extras += (f'<hp:p {P_ATTRS.format(id=pid)}><hp:run charPrIDRef="0">'
                       f'{content}<hp:t/></hp:run></hp:p>')
    return section.replace('</hs:sec>', extras + '</hs:sec>')


def synth_png(width: int, height: int, rgb: bool) -> bytes:
    """합성 유효 PNG (gen_editing.py 와 같은 규칙: IHDR/IDAT/IEND, 결정적)."""
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2 if rgb else 0, 0, 0, 0)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    stride = b""
    for _ in range(height):
        stride += b"\x00" + (b"\x20\x40\x60" if rgb else b"\x7f") * width
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(stride, 9))
        + chunk(b"IEND", b"")
    )


def build_hwpx(dest: Path, original_png: bytes, *, table: bool = True, picture: bool = True) -> None:
    """Build from a public native package, preserving complete header/defaults.

    The generated text/media are synthetic. Native compatibility remains not_checked
    until these exact bytes are opened and resaved on Windows.
    """
    with zipfile.ZipFile(PUBLIC_SEED) as seed:
        files = {i.filename: (seed.read(i), i.compress_type) for i in seed.infolist()}
    header = files["Contents/header.xml"][0].decode("utf-8")
    header = header.replace('<hh:borderFills itemCnt="2">', '<hh:borderFills itemCnt="3">')
    header = header.replace('</hh:borderFills>', BORDER_FILL + '</hh:borderFills>')
    files["Contents/header.xml"] = (header.encode(), 8)
    files["Contents/section0.xml"] = (section_xml(table=table, picture=picture).encode(), 8)
    # Remove public author/date metadata; do not copy private document metadata.
    hpf = files["Contents/content.hpf"][0].decode("utf-8")
    hpf = re.sub(r'<opf:metadata>.*?</opf:metadata>',
                 '<opf:metadata><opf:title>Engine evaluation</opf:title><opf:language>ko</opf:language></opf:metadata>',
                 hpf, flags=re.S)
    if picture:
        hpf = hpf.replace('</opf:manifest>',
                          '<opf:item id="image1" href="BinData/image1.png" media-type="image/png" isEmbeded="1"/></opf:manifest>')
        files["BinData/image1.png"] = (original_png, 0)
    files["Contents/content.hpf"] = (hpf.encode(), 8)
    files["Preview/PrvText.txt"] = ("평가 문단 하나\r\n".encode("utf-8"), 8)
    with zipfile.ZipFile(dest, "w") as z:
        for name, (data, method) in files.items():
            zi = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            zi.compress_type = method
            zi.create_system = 0
            zi.external_attr = 0o100644 << 16
            z.writestr(zi, data, compress_type=method, compresslevel=9 if method == 8 else None)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def payload_set(hwpx_bytes: bytes) -> dict:
    """ZIP 항목별 해시(무편집 항목 압축 데이터 보존 확인용)."""
    out = {}
    with zipfile.ZipFile(io.BytesIO(hwpx_bytes)) as z:
        for info in z.infolist():
            out[info.filename] = sha256_bytes(z.read(info.filename))
    return out


def png_ink_bbox(data: bytes):
    """PNG(8비트, 비인터레이스) 의 비백색 픽셀 경계 상자와 잉크 비율. 표준 라이브러리만 쓴다.

    반환: (width, height, (x0, y0, x1, y1) 또는 None, ink_ratio). 밝기 200 미만을 잉크로 본다.
    """
    pos, idat, width, height, ctype, depth, interlace = 8, b"", 0, 0, 0, 0, 0
    while pos < len(data):
        (n,) = struct.unpack(">I", data[pos : pos + 4])
        tag, body = data[pos + 4 : pos + 8], data[pos + 8 : pos + 8 + n]
        pos += 12 + n
        if tag == b"IHDR":
            width, height, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
        elif tag == b"IDAT":
            idat += body
    if depth != 8 or interlace != 0 or ctype not in (0, 2, 4, 6):
        raise ValueError(f"unsupported png: depth={depth} color={ctype} interlace={interlace}")
    bpp = {0: 1, 2: 3, 4: 2, 6: 4}[ctype]
    stride = width * bpp
    raw, prev, i = zlib.decompress(idat), bytearray(stride), 0
    x0 = y0 = None
    x1 = y1 = -1
    ink = 0
    for y in range(height):
        ft, line = raw[i], bytearray(raw[i + 1 : i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if ft == 1:
                line[x] = (line[x] + a) & 255
            elif ft == 2:
                line[x] = (line[x] + b) & 255
            elif ft == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif ft == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = line
        color = 1 if ctype in (0, 4) else 3
        for x in range(width):
            px = line[x * bpp : x * bpp + color]
            alpha = line[x * bpp + color] if ctype in (4, 6) else 255
            if alpha > 0 and min(px) < 200:
                ink += 1
                x0 = x if x0 is None else min(x0, x)
                x1, y1 = max(x1, x), y
                y0 = y if y0 is None else y0
    bbox = None if x0 is None else (x0, y0, x1, y1)
    return width, height, bbox, ink / max(width * height, 1)


class Fatal(Exception):
    pass


class McpClient:
    """stdio MCP 서버를 실제 프로세스로 띄우고 JSON-RPC 한 줄 요청/응답을 주고받는다.

    `bufsize=0` 으로 두고 `os.read` 로 직접 프레이밍한다. BufferedReader 의 선반입
    버퍼 때문에 `select` 가 미도착으로 보이면서 image 가 멈추는 문제를 피한다.
    stderr 는 별도 스레드에서 계속 비워 파이프가 막히지 않게 한다.
    """

    MAX_LINE_BYTES = 4 * 1024 * 1024
    MAX_STDERR_BYTES = 8 * 1024 * 1024

    def __init__(self, binary: Path, roots: Path, rhwp: Path, bwrap: Path, font_dir: Path,
                 timeout: float, log, raster=None):
        self.log = log
        self.timeout = timeout
        self._inbuf = bytearray()
        self._stderr = bytearray()
        env = dict(os.environ)
        env["HAE_RHWP"] = str(rhwp)
        env["HAE_BWRAP"] = str(bwrap)
        env["HAE_FONT_DIR"] = str(font_dir)
        env["HAE_RASTER"] = str(raster or binary.with_name("hae-raster"))
        self.proc = subprocess.Popen(
            [str(binary), "--input-root", str(roots), "--output-root", str(roots)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            bufsize=0,
        )

        def drain_stderr():
            while True:
                chunk = self.proc.stderr.read(65536)
                if not chunk:
                    return
                if len(self._stderr) < self.MAX_STDERR_BYTES:
                    self._stderr.extend(chunk)

        self._stderr_thread = threading.Thread(target=drain_stderr, daemon=True)
        self._stderr_thread.start()
        self.next_id = 1
        self.rpc(
            "initialize",
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "hae-evaluate", "version": "0"},
            },
        )
        self.notify("notifications/initialized", {})

    def _read_line(self) -> bytes:
        deadline = time.monotonic() + self.timeout
        while True:
            nl = self._inbuf.find(b"\n")
            if nl >= 0:
                if nl > self.MAX_LINE_BYTES:
                    raise Fatal(f"MCP 응답 한 줄이 {self.MAX_LINE_BYTES} 바이트를 넘었다")
                line = bytes(self._inbuf[:nl])
                del self._inbuf[: nl + 1]
                return line
            if len(self._inbuf) > self.MAX_LINE_BYTES:
                raise Fatal(f"MCP 응답 한 줄이 {self.MAX_LINE_BYTES} 바이트를 넘었다")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise Fatal(f"MCP 서버 응답 시간 초과({self.timeout}s)")
            ready, _, _ = select.select([self.proc.stdout], [], [], remaining)
            if not ready:
                continue
            chunk = os.read(self.proc.stdout.fileno(), 65536)
            if not chunk:
                raise Fatal(f"MCP 서버 stdout 이 닫혔다. stderr: {self.stderr_text()[:600]}")
            self._inbuf.extend(chunk)

    def send(self, message: dict) -> None:
        self.proc.stdin.write(json.dumps(message, ensure_ascii=False).encode() + b"\n")
        self.proc.stdin.flush()

    def notify(self, method: str, params: dict) -> None:
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def rpc(self, method: str, params: dict) -> dict:
        rid = self.next_id
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        reply = json.loads(self._read_line().decode("utf-8"))
        if reply.get("id") != rid:
            raise Fatal(f"JSON-RPC id 불일치: {reply.get('id')} != {rid}")
        return reply

    def call(self, tool: str, arguments: dict) -> tuple:
        reply = self.rpc("tools/call", {"name": tool, "arguments": arguments})
        result = reply.get("result")
        if not isinstance(result, dict):
            raise Fatal(f"tools/call 프로토콜 오류: {json.dumps(reply, ensure_ascii=False)[:400]}")
        text = result["content"][0]["text"]
        envelope = json.loads(text)
        is_error = bool(result.get("isError"))
        self.log({"kind": "tool", "tool": tool, "arguments": arguments, "isError": is_error, "response": envelope})
        return envelope, is_error

    def stderr_text(self) -> str:
        return bytes(self._stderr).decode("utf-8", "replace")

    def close(self) -> str:
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.wait(timeout=15)
        except Exception:
            self.proc.kill()
        self._stderr_thread.join(timeout=5)
        return self.stderr_text()


class Runner:
    def __init__(self, args):
        self.args = args
        self.out = Path(args.output_dir).resolve()
        self.logs = []
        self.assertions = []
        self.notes = []
        self.hashes = {}
        self.started = time.time()
        self.artifacts = {}
        self.ready = False

    # -- assertion helpers -------------------------------------------------
    def check(self, name: str, ok: bool, detail=None) -> bool:
        self.assertions.append({"name": name, "ok": bool(ok), "detail": detail})
        if not ok:
            print(f"FAIL  {name}: {detail}", file=sys.stderr, flush=True)
        return bool(ok)

    def require(self, name: str, ok: bool, detail=None) -> None:
        if not self.check(name, ok, detail):
            raise Fatal(f"{name}: {detail}")

    def log(self, entry: dict) -> None:
        entry = dict(entry)
        entry["ts"] = round(time.time() - self.started, 3)
        self.logs.append(entry)

    def note(self, name: str, detail) -> None:
        self.notes.append({"name": name, "detail": detail})

    # -- setup -------------------------------------------------------------
    def prepare_dirs(self) -> None:
        if self.out.exists():
            if any(self.out.iterdir()):
                raise Fatal(f"출력 디렉터리가 비어 있지 않다(기존 근거 보호): {self.out}")
        else:
            self.out.mkdir(parents=True)
        self.inputs = self.out / "inputs"
        self.artifacts_dir = self.out / "artifacts"
        self.logs_dir = self.out / "logs"
        for d in (self.inputs, self.artifacts_dir, self.logs_dir):
            d.mkdir()
        self.ready = True

    def sha(self, path: Path, key: str) -> str:
        digest = sha256_file(path)
        self.hashes[key] = {"path": str(path), "sha256": digest, "bytes": path.stat().st_size}
        return digest

    # -- main flow ---------------------------------------------------------
    def run(self) -> None:
        self.prepare_dirs()

        binary = Path(self.args.binary).resolve()
        rhwp = Path(self.args.rhwp).resolve()
        bwrap = Path(self.args.bwrap).resolve()
        self.require("engine_binary_exists", binary.is_file(), str(binary))
        self.require("renderer_binary_exists", rhwp.is_file(), str(rhwp))
        self.require("bwrap_exists", bwrap.is_file(), str(bwrap))
        raster = Path(self.args.raster).resolve() if self.args.raster else binary.with_name("hae-raster")
        self.require("raster_worker_exists", raster.is_file(), str(raster))
        self.sha(raster, "raster_worker")
        binary_sha = sha256_file(binary)
        rhwp_sha = sha256_file(rhwp)
        self.hashes["engine_binary"] = {"path": str(binary), "sha256": binary_sha}
        self.hashes["renderer_binary"] = {"path": str(rhwp), "sha256": rhwp_sha}
        self.note("renderer_binary_stage3_record_match",
                  {"recorded_sha256": "e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7",
                   "observed_sha256": rhwp_sha,
                   "matches": rhwp_sha == "e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7"})

        # 시험 문서·교체 이미지 준비 (모두 출력 루트 아래)
        doc_path = self.inputs / "eval-document.hwpx"
        original_png = synth_png(1, 1, rgb=False)
        build_hwpx(doc_path, original_png)
        doc_sha = self.sha(doc_path, "document")
        (self.inputs / "original-media.png").write_bytes(original_png)
        self.sha(self.inputs / "original-media.png", "original_media")

        replacement_src = REPO_ROOT / "tests/fixtures/editing/inputs/ok.png"
        self.require("replacement_input_exists", replacement_src.is_file(), str(replacement_src))
        replacement = self.inputs / "replacement-ok.png"
        shutil.copyfile(replacement_src, replacement)
        replacement_sha = self.sha(replacement, "replacement_before")

        # 렌더러 폰트 예산을 지키기 위해 작은 전용 폰트 폴더를 만든다(기본 /usr/share/fonts 는 너무 큼).
        font_src = Path(self.args.font_source).resolve()
        self.require("font_source_exists", font_src.is_file(), str(font_src))
        font_dir = self.inputs / "fonts"
        font_dir.mkdir()
        font_dst = font_dir / font_src.name
        shutil.copyfile(font_src, font_dst)
        self.sha(font_dst, "font")
        self.note("font_dir", {"dir": str(font_dir), "files": sorted(p.name for p in font_dir.iterdir())})

        client = McpClient(binary, self.out, rhwp, bwrap, font_dir, float(self.args.timeout), self.log, raster=raster)
        stderr = ""
        try:
            self.verify_tools_schema(client)
            self.evaluate(client, doc_path, doc_sha, replacement, replacement_sha)
        finally:
            stderr = client.close()
        (self.logs_dir / "engine-stderr.log").write_text(stderr, encoding="utf-8")

    def verify_tools_schema(self, client: McpClient) -> None:
        reply = client.rpc("tools/list", {})
        tools = {t["name"]: t for t in reply["result"]["tools"]}
        self.log({"kind": "tools/list", "count": len(tools),
                  "names": sorted(tools)})
        expected = {
            "document_open", "document_close", "document_inspect", "element_query", "element_get",
            "text_replace", "paragraph_format", "cell_set_text", "image_replace", "command_batch",
            "history_undo", "history_redo", "command_list", "command_describe",
            "document_snapshot", "snapshot_release", "document_diff",
            "document_preview", "project_save", "document_export",
        }
        missing = sorted(expected - set(tools))
        self.require("tools_list_has_expected", not missing, f"missing={missing}")

        def required(name):
            return tools[name]["inputSchema"].get("required", [])

        def props(name):
            return sorted(tools[name]["inputSchema"].get("properties", {}).keys())

        # documentId 는 열린 문서가 하나면 생략할 수 있어(선택 인자) required 가 아니라 properties 에서 확인한다.
        has_doc_id = all("documentId" in props(n) for n in (
            "command_batch", "cell_set_text", "paragraph_format", "image_replace",
            "document_export", "snapshot_release"))
        schema_ok = (
            "path" in required("document_open")
            and has_doc_id
            and {"steps", "expectedVersion"} <= set(required("command_batch"))
            and "dryRun" in props("command_batch")
            and {"target", "text", "expectedVersion"} <= set(required("cell_set_text"))
            and {"target", "expectedVersion"} <= set(required("paragraph_format"))
            and {"target", "imagePath", "expectedVersion"} <= set(required("image_replace"))
            and {"pages", "includePng", "includeSvg"} <= set(props("document_preview"))
            and {"path"} <= set(required("document_export"))
            and {"snapshotId"} <= set(required("snapshot_release"))
        )
        self.check("tools_input_schemas", schema_ok,
                   {n: {"required": required(n), "properties": props(n)}
                    for n in ["document_open", "command_batch", "cell_set_text", "paragraph_format",
                              "image_replace", "document_preview", "document_export"]})
        self.check("query_tools_read_only_hint",
                   all(tools[t]["annotations"]["readOnlyHint"] for t in ("element_query", "element_get", "document_inspect")),
                   None)

    def evaluate(self, client, doc_path, doc_sha, replacement, replacement_sha) -> None:
        # 1) open
        opened, opened_err = client.call("document_open", {"path": str(doc_path)})
        self.require("open_ok", opened.get("ok") and not opened_err, opened)
        doc = opened["result"]["documentId"]
        self.check("open_revision_zero", opened["result"]["revision"] == 0, opened["result"])
        self.check("open_editable", opened["result"]["editable"] is True,
                   opened["result"].get("readonlyReasons"))
        self.check("open_readonly_reasons_empty", opened["result"].get("readonlyReasons") == [],
                   opened["result"].get("readonlyReasons"))

        # 2) snapshot(before)
        snap, _ = client.call("document_snapshot", {"documentId": doc, "label": "before-edits"})
        self.require("snapshot_ok", snap.get("ok") is True, snap)
        base_snap = snap["result"]["snapshotId"]

        # 3) preview before
        before = self.preview(client, doc, 0, "before")

        # 4) 명시 대상 조회(유일)
        tables, _ = client.call("element_query", {"documentId": doc, "kind": "table"})
        self.require("query_table_unique", len(tables["result"]["matches"]) == 1, tables["result"]["matches"])
        table = tables["result"]["matches"][0]["nodeId"]
        cell_q, _ = client.call("element_query", {"documentId": doc, "table": table, "row": 0, "col": 0})
        self.require("query_cell_unique", len(cell_q["result"]["matches"]) == 1, cell_q["result"]["matches"])
        cell = cell_q["result"]["matches"][0]
        self.require("cell_editable_single_paragraph",
                     cell["editable"] and cell["cell"]["rowSpan"] == 1 and cell["cell"]["colSpan"] == 1,
                     cell)
        para_q, _ = client.call("element_query", {"documentId": doc, "kind": "paragraph", "textContains": "평가 문단"})
        self.require("query_paragraph_unique", len(para_q["result"]["matches"]) == 1, para_q["result"]["matches"])
        para = para_q["result"]["matches"][0]
        self.require("paragraph_editable", para["editable"], para)
        pic_q, _ = client.call("element_query", {"documentId": doc, "kind": "picture"})
        self.require("query_picture_unique", len(pic_q["result"]["matches"]) == 1, pic_q["result"]["matches"])
        pic = pic_q["result"]["matches"][0]
        self.require("picture_uncropped_no_effects", pic["editable"] and pic.get("notEditableReason") is None, pic)
        self.targets = {"cell": cell["nodeId"], "paragraph": para["nodeId"], "picture": pic["nodeId"],
                        "table": table, "picture_media": pic.get("media")}

        # 5) 원본 무편집 내보내기 = 원본 파일 바이트 그대로
        orig_export = self.artifacts_dir / "original-export.hwpx"
        ex, _ = client.call("document_export", {"documentId": doc, "path": str(orig_export)})
        self.require("original_export_ok", ex.get("ok") is True, ex)
        self.check("original_export_unmodified_passthrough", ex["result"]["unmodifiedPassthrough"] is True, ex["result"])
        self.check("original_export_version_zero", ex["result"]["exportedVersion"] == 0, ex["result"])
        self.require("original_export_bytes_exact", sha256_file(orig_export) == doc_sha,
                     {"reported": ex["result"]["sha256"], "actual": sha256_file(orig_export), "document": doc_sha})
        self.sha(orig_export, "export_original")

        # 6) dryRun batch (셀 텍스트 + 문단 정렬 + 그림 바이트)
        steps = [
            {"command": "cell.setText", "params": {"target": self.targets["cell"], "text": "바뀐셀"}},
            {"command": "paragraph.format",
             "params": {"target": self.targets["paragraph"], "alignment": "center",
                        "lineSpacing": {"kind": "percent", "value": 180}}},
            {"command": "image.replace", "params": {"target": self.targets["picture"], "imagePath": str(replacement)}},
        ]
        dry, _ = client.call("command_batch",
                             {"documentId": doc, "expectedVersion": 0, "dryRun": True, "steps": steps})
        self.check("dryrun_ok", dry.get("ok") is True, dry)
        if dry.get("ok"):
            r = dry["result"]
            self.check("dryrun_outcome_applied", r["outcome"] == "applied", r)
            self.check("dryrun_revision_unchanged", r["beforeVersion"] == 0 and r["afterVersion"] == 0, r)
            self.check("dryrun_predicted_version", r.get("predictedAfterVersion") == 1, r)

        # dryRun 이 상태를 바꾸지 않았는지: 현재 revision 0 유지 + diff 동일
        inspected, _ = client.call("document_inspect", {"documentId": doc, "limit": 1})
        self.check("revision_still_zero_after_dryrun", inspected["result"]["revision"] == 0, inspected["result"])
        pre_diff, _ = client.call("document_diff", {"documentId": doc})
        self.check("dryrun_state_unchanged", pre_diff["result"]["identical"] is True, pre_diff["result"])

        # 7) batch 적용: revision 정확히 +1
        applied, _ = client.call("command_batch",
                                 {"documentId": doc, "expectedVersion": 0, "steps": steps})
        self.require("apply_ok", applied.get("ok") is True, applied)
        ar = applied["result"]
        self.check("apply_outcome_applied", ar["outcome"] == "applied", ar)
        self.check("apply_revision_plus_one_once", ar["beforeVersion"] == 0 and ar["afterVersion"] == 1, ar)
        self.check("apply_changed_ids_cover_targets",
                   {self.targets["cell"], self.targets["paragraph"], self.targets["picture"]} <= set(ar["changedIds"]),
                   ar["changedIds"])
        after_state, _ = client.call("document_inspect", {"documentId": doc, "limit": 1})
        self.check("revision_is_one_after_apply", after_state["result"]["revision"] == 1, after_state["result"])

        applied_export = self.artifacts_dir / "applied-export.hwpx"
        ax, _ = client.call("document_export", {"documentId": doc, "path": str(applied_export)})
        self.require("applied_export_ok", ax.get("ok") is True, ax)
        self.check("applied_export_version_one", ax["result"]["exportedVersion"] == 1, ax["result"])
        self.check("applied_export_differs_from_original", sha256_file(applied_export) != doc_sha, None)
        self.sha(applied_export, "export_applied")
        applied_payloads = payload_set(applied_export.read_bytes())

        # 8) diff (base snapshot vs 현재)
        diff, _ = client.call("document_diff", {"documentId": doc, "baseSnapshotId": base_snap})
        dr = diff["result"]
        self.check("diff_not_identical", dr["identical"] is False, dr)
        node_by_id = {n["nodeId"]: n for n in dr.get("changedNodes", [])}
        cell_change = node_by_id.get(self.targets["cell"])
        pic_change = node_by_id.get(self.targets["picture"])
        self.check("diff_cell_text_changed", bool(cell_change) and cell_change["afterText"] == "바뀐셀", cell_change)
        self.check("diff_picture_media_changed",
                   bool(pic_change) and pic_change["beforeMedia"] != pic_change["afterMedia"], pic_change)
        self.check("diff_media_changes_present", len(dr.get("mediaChanges", [])) >= 1, dr.get("mediaChanges"))
        self.check("diff_section_entry_changed", "Contents/section0.xml" in dr.get("changedEntries", []),
                   dr.get("changedEntries"))
        self.check("diff_header_entry_changed", "Contents/header.xml" in dr.get("changedEntries", []),
                   dr.get("changedEntries"))

        # 9) preview after
        after = self.preview(client, doc, 1, "after")

        # 10) undo → revision 2, 내보내기 = 원본 파일 바이트 exact
        undo, _ = client.call("history_undo", {"documentId": doc, "expectedVersion": 1})
        self.require("undo_ok", undo.get("ok") is True, undo)
        self.check("undo_after_version_two", undo["result"]["afterVersion"] == 2, undo["result"])
        undo_export = self.artifacts_dir / "undo-export.hwpx"
        ux, _ = client.call("document_export", {"documentId": doc, "path": str(undo_export)})
        self.require("undo_export_ok", ux.get("ok") is True, ux)
        self.check("undo_export_version_two", ux["result"]["exportedVersion"] == 2, ux["result"])
        self.check("undo_export_passthrough_true", ux["result"]["unmodifiedPassthrough"] is True, ux["result"])
        self.require("undo_export_original_bytes_exact", sha256_file(undo_export) == doc_sha,
                     {"reported": ux["result"]["sha256"], "actual": sha256_file(undo_export), "document": doc_sha})
        self.sha(undo_export, "export_undo")

        # 11) 교체 원본 파일 변조(redo 가 경로를 다시 읽지 않는지 확인)
        replacement.write_bytes(b"corrupted-after-apply")
        replacement_after_sha = self.sha(replacement, "replacement_after_corruption")
        self.check("replacement_source_changed", replacement_after_sha != replacement_sha,
                   {"before": replacement_sha, "after": replacement_after_sha})

        # 12) redo → revision 3, payload 가 적용 결과와 동일
        redo, _ = client.call("history_redo", {"documentId": doc, "expectedVersion": 2})
        self.require("redo_ok", redo.get("ok") is True, redo)
        self.check("redo_after_version_three", redo["result"]["afterVersion"] == 3, redo["result"])
        redo_export = self.artifacts_dir / "redo-export.hwpx"
        rx, _ = client.call("document_export", {"documentId": doc, "path": str(redo_export)})
        self.require("redo_export_ok", rx.get("ok") is True, rx)
        self.check("redo_export_version_three", rx["result"]["exportedVersion"] == 3, rx["result"])
        self.check("redo_export_matches_applied_bytes", sha256_file(redo_export) == sha256_file(applied_export),
                   {"redo": sha256_file(redo_export), "applied": sha256_file(applied_export)})
        self.check("redo_payloads_match_applied", payload_set(redo_export.read_bytes()) == applied_payloads, None)
        self.sha(redo_export, "export_redo")

        # 13) project 저장 → 편집 export → close → project 재열기(ID·revision 유지)
        project = self.artifacts_dir / "session.hwpwork"
        ps, _ = client.call("project_save", {"documentId": doc, "path": str(project)})
        self.require("project_save_ok", ps.get("ok") is True, ps)
        self.check("project_saved_version_three", ps["result"]["savedVersion"] == 3, ps["result"])
        self.check("project_file_exists", project.is_file(), str(project))
        if project.is_file():
            self.sha(project, "project_file")

        edited_export = self.artifacts_dir / "edited-export.hwpx"
        exf, _ = client.call("document_export", {"documentId": doc, "path": str(edited_export)})
        self.require("edited_export_ok", exf.get("ok") is True, exf)
        self.check("edited_export_version_three", exf["result"]["exportedVersion"] == 3, exf["result"])
        self.check("edited_export_not_passthrough", exf["result"]["unmodifiedPassthrough"] is False, exf["result"])
        self.sha(edited_export, "export_edited")

        closed, _ = client.call("document_close", {})
        self.require("close_ok", closed.get("ok") is True, closed)

        reopened, reopened_err = client.call("document_open", {"path": str(project)})
        self.require("reopen_project_ok", reopened.get("ok") is True and not reopened_err, reopened)
        self.check("reopen_project_id_stable", reopened["result"]["documentId"] == doc,
                   {"before": doc, "after": reopened["result"]["documentId"]})
        self.check("reopen_project_revision_stable", reopened["result"]["revision"] == 3, reopened["result"])

        # §13 ID 지속: 내보낸 HWPX 를 새로 열면 새 문서 ID
        client.call("document_close", {})
        exported_open, _ = client.call("document_open", {"path": str(edited_export)})
        self.require("reopen_exported_ok", exported_open.get("ok") is True, exported_open)
        self.check("exported_hwpx_new_document_id",
                   exported_open["result"]["documentId"] != doc, {"project": doc, "exported": exported_open["result"]["documentId"]})

        self.artifacts["preview_before"] = before
        self.artifacts["preview_after"] = after

    def layout_checks(self, tag, png_raw, svg) -> None:
        """조판 회귀 방지: 쪽 설정이 있는 보통 문서는 첫 쪽 위·왼쪽 가장자리에 내용이 닿지 않는다.

        쪽 설정(secPr/pagePr)이 빠진 합성 문서는 내용이 상단 약 15px 에 몰리고 글자가 y=0 에 닿았다.
        """
        width, height, bbox, ink = png_ink_bbox(png_raw)
        self.note(f"preview_{tag}_layout", {"size": [width, height], "inkBBox": bbox, "inkRatio": round(ink, 5)})
        self.check(f"preview_{tag}_png_has_ink", bbox is not None, None)
        if bbox is None:
            return
        x0, y0, x1, y1 = bbox
        margin = max(10, round(min(width, height) * 0.02))
        self.check(f"preview_{tag}_png_content_off_edges",
                   x0 >= margin and y0 >= margin and x1 <= width - 1 - margin and y1 <= height - 1 - margin,
                   {"bbox": bbox, "size": [width, height], "minMargin": margin})
        self.check(f"preview_{tag}_png_content_spans_page_body", y1 - y0 >= height * 0.1,
                   {"inkHeight": y1 - y0, "pageHeight": height})
        if svg:
            m = re.match(r'<svg[^>]*\swidth="([\d.]+)"[^>]*\sheight="([\d.]+)"[^>]*\sviewBox="0 0 ([\d.]+) ([\d.]+)"', svg)
            vb_ok = bool(m) and abs(float(m[1]) - float(m[3])) < 0.01 and abs(float(m[2]) - float(m[4])) < 0.01
            self.check(f"preview_{tag}_svg_viewbox_equals_size", vb_ok, m.groups() if m else svg[:200])

    def preview(self, client, doc, revision, tag) -> dict:
        resp, err = client.call(
            "document_preview",
            {"documentId": doc, "pages": [1], "includePng": True, "includeSvg": True},
        )
        ok = resp.get("ok") is True and not err
        self.check(f"preview_{tag}_ok", ok, resp if not ok else None)
        out = {"ok": ok, "revision_expected": revision}
        if not ok:
            return out
        r = resp["result"]
        out["status"] = r["status"]
        out["revision"] = r["revision"]
        out["pageCount"] = r["pageCount"]
        out["coverage"] = r.get("coverage")
        out["metadata"] = r.get("metadata")
        self.check(f"preview_{tag}_revision", r["revision"] == revision, {"expected": revision, "actual": r["revision"]})
        self.check(f"preview_{tag}_status_rendered",
                   r["status"] in ("rendered", "partial"), r["status"])
        self.check(f"preview_{tag}_page_count", r.get("pageCount", 0) >= 1, r.get("pageCount"))
        meta = r.get("metadata") or {}
        self.check(f"preview_{tag}_native_comparison_not_checked",
                   meta.get("nativeComparison") == "not_checked", meta.get("nativeComparison"))
        pages = r.get("pages") or []
        if pages:
            page = pages[0]
            png = ((page.get("png") or {}).get("base64"))
            svg = page.get("svg")
            if png:
                raw = base64.b64decode(png)
                self.check(f"preview_{tag}_png_bounded", len(raw) <= MAX_INLINE_BYTES, len(raw))
                p = self.artifacts_dir / f"preview-{tag}-page1.png"
                p.write_bytes(raw)
                self.sha(p, f"preview_{tag}_png")
                self.layout_checks(tag, raw, svg)
                out["png"] = {"path": str(p), "bytes": len(raw), "sha256": sha256_bytes(raw)}
            if svg:
                raw = svg.encode("utf-8")
                self.check(f"preview_{tag}_svg_bounded", len(raw) <= MAX_INLINE_BYTES, len(raw))
                p = self.artifacts_dir / f"preview-{tag}-page1.svg"
                p.write_bytes(raw)
                self.sha(p, f"preview_{tag}_svg")
                out["svg"] = {"path": str(p), "bytes": len(raw), "sha256": sha256_bytes(raw)}
            self.check(f"preview_{tag}_has_png_or_svg", bool(png or svg), {"png": bool(png), "svg": bool(svg)})
        return out


def write_summary(runner, status, failure=None):
    elapsed = round(time.time() - runner.started, 3)
    renderer_meta = None
    for key in ("preview_before", "preview_after"):
        prev = runner.artifacts.get(key)
        if prev and prev.get("metadata", {}).get("renderer"):
            renderer_meta = prev["metadata"]["renderer"]
            break
    summary = {
        "schemaVersion": 1,
        "status": status,
        "startedAt": time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime(runner.started)),
        "elapsedSeconds": elapsed,
        "nativeComparison": "not_checked",
        "engine": {
            "binary": runner.hashes.get("engine_binary", {}).get("path"),
            "sha256": runner.hashes.get("engine_binary", {}).get("sha256"),
        },
        "renderer": {
            "rhwpPath": runner.hashes.get("renderer_binary", {}).get("path"),
            "rhwpSha256": runner.hashes.get("renderer_binary", {}).get("sha256"),
            "rasterWorker": runner.hashes.get("raster_worker"),
            "bwrap": str(Path(runner.args.bwrap).resolve()),
            "font": runner.hashes.get("font"),
            "metadata": renderer_meta,
        },
        "document": {
            "kind": "synthetic-combined",
            "notes": ("합성 문서: 문단 하나 + 병합 없는 단일 문단 셀 2개 + 자르기·효과 없는 그림 하나. "
                      "공개 MIT ref_empty.hwpx 패키지·헤더·구역 기본값과 완전한 그림 필드를 사용하며, "
                      "교체 이미지는 tests/fixtures/editing/inputs/ok.png 를 쓴다. 개인 문서를 쓰지 않는다."),
            "path": runner.hashes.get("document", {}).get("path"),
            "sha256": runner.hashes.get("document", {}).get("sha256"),
            "replacementBeforeSha256": runner.hashes.get("replacement_before", {}).get("sha256"),
            "replacementAfterCorruptionSha256": runner.hashes.get("replacement_after_corruption", {}).get("sha256"),
        },
        "assertions": runner.assertions,
        "assertionsPassed": sum(1 for a in runner.assertions if a["ok"]),
        "assertionsFailed": sum(1 for a in runner.assertions if not a["ok"]),
        "notes": runner.notes,
        "hashes": runner.hashes,
        "preview": runner.artifacts,
        "failure": failure,
    }
    (runner.out / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    (runner.out / "hashes.json").write_text(
        json.dumps(runner.hashes, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    with (runner.out / "logs" / "responses.jsonl").open("w", encoding="utf-8") as fh:
        for entry in runner.logs:
            fh.write(json.dumps(entry, ensure_ascii=False) + "\n")
    return summary


def parse_args(argv):
    parser = argparse.ArgumentParser(
        prog="evaluate_mcp.py",
        description=("Linux 공개/합성 자료로 HWPX 엔진의 대표 시나리오(셀 텍스트·문단 정렬·그림 교체)를 "
                     "실제 stdio MCP 서버에서 실행하고 판정·로그·해시를 남긴다. 한컴 호환은 주장하지 않는다."),
        epilog="출력 디렉터리는 비어 있어야 한다(기존 근거 보호). 매 실행 새 경로를 쓴다.",
    )
    parser.add_argument("--output-dir", required=True,
                        help="판정·로그·산출물을 쓸 새 디렉터리(비어 있어야 함)")
    parser.add_argument("--binary", default=str(REPO_ROOT / "target/debug/hae-mcp"),
                        help="hae-mcp 실행 파일 경로(기본: target/debug/hae-mcp)")
    parser.add_argument("--rhwp", default=str(REPO_ROOT / "spikes/vendor/rhwp-src/target/release/rhwp"),
                        help="격리 렌더러 rhwp 경로")
    parser.add_argument("--raster", help="hae-raster PNG 작업 실행 파일(기본: hae-mcp와 같은 폴더)")
    parser.add_argument("--bwrap", default="/usr/bin/bwrap", help="bubblewrap 경로")
    parser.add_argument("--font-source", default="/usr/local/share/fonts/n/NanumGothic.ttf",
                        help="평가용 폰트 폴더에 복사할 폰트 파일(기본: NanumGothic.ttf)")
    parser.add_argument("--timeout", type=float, default=180.0, help="MCP 응답 대기 상한(초, 기본 180)")
    return parser.parse_args(argv)


def main(argv=None):
    args = parse_args(sys.argv[1:] if argv is None else argv)
    runner = Runner(args)
    status = "passed"
    failure = None
    try:
        runner.run()
    except Fatal as exc:
        status = "failed"
        failure = {"kind": "fatal", "message": str(exc)}
        runner.check(f"fatal_{str(exc)[:60]}", False, str(exc))
    except Exception as exc:  # noqa: BLE001 - 어떤 실패도 summary 를 남긴다
        status = "failed"
        failure = {"kind": type(exc).__name__, "message": str(exc)}
        runner.check(f"exception_{type(exc).__name__}", False, str(exc))
    if runner.assertions and any(not a["ok"] for a in runner.assertions):
        status = "failed"
    if failure and failure.get("kind") == "fatal" and not runner.ready:
        # 준비 단계 실패(예: 비어 있지 않은 출력 디렉터리): 기존 근거를 건드리지 않고 종료.
        print(f"evaluation aborted before setup: {failure['message']}", file=sys.stderr, flush=True)
        return 2
    try:
        summary = write_summary(runner, status, failure)
        print(f"summary: {runner.out / 'summary.json'}", flush=True)
        print(f"status={summary['status']} passed={summary['assertionsPassed']} "
              f"failed={summary['assertionsFailed']}", flush=True)
    except Exception as exc:  # summary 를 못 써도 실패 코드
        print(f"failed to write summary: {exc}", file=sys.stderr, flush=True)
        return 1
    return 0 if status == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())

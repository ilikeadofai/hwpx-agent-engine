# Hancom HWP SDK Trial 검토 — 공개 바인딩·약관·클린룸 인계

검토일: 2026-10-17  
담당: SDK 분석 (GPT 6 Luna)  
판정 범위: 제공된 ZIP의 텍스트 고지, Python 공개 바인딩·샘플, XML 설정 구조와 선별된 PE 헤더/버전 리소스. 법률 자문이나 SDK 실행 결과가 아니다.

## 1. 검토 방법과 자료 경계

- SDK trial ZIP과 LicenseManager ZIP은 사용자 로컬 자료로만 읽었다. SDK ZIP의 일반 파일 246개(압축 해제 기준 122,409,796 bytes), LicenseManager ZIP의 일반 파일 2개(73,211,154 bytes)를 SHA-256/크기 목록으로 남겼다.
- 텍스트로는 README.md, 두 Notice 파일, Bin/hwpsdk.py, Bin64/hwpsdk.py, Sample/Python/HwpSDKSample.py만 추출했다. PE 메타데이터 확인용으로 DLL 6개와 LicenseManager EXE 1개도 같은 격리 경로에 두었다. ProductConfig.xml은 ZIP에서 XML 구조만 읽었고 원문이나 필드 값을 저장하지 않았다.
- API 소스와 샘플은 정적으로 읽었으며 import하거나 실행하지 않았다. SDK DLL과 LicenseManager EXE를 실행·디스어셈블·디컴파일하지 않았고, strings도 사용하지 않았다. PE 검사는 선별된 DLL/EXE의 아키텍처 헤더와 버전 리소스만 확인했다.
- 네트워크를 사용하지 않았고, 엔진 구현 소스 crates/, scripts/는 열거나 수정하지 않았다. 원본 ZIP/첨부를 수정하거나 외부에 재배포하지 않았다. 추출 파일과 인벤토리는 git 무시 경로에만 두었다.

## 2. SDK 사용 약관 판정

**상태: 미확인. 제공된 패키지로 SDK 자체의 사용 약관을 판정할 수 없다.**

고지에서 확인한 짧은 원문:

- Hwp_Sdk_Notice.txt: “Hancom Open Source Software Notice” (제품/모듈 표기는 Hancom HWP SDK V1.0, Hwp_Sdk)
- README.md: “This software uses Open Source Software (OSS).”
- README.md: “Copyrights © Hancom Inc. All rights reserved.”
- hwp-2024_Notice.txt도 Hancom HWP SDK V1.0의 hwp 모듈에 대한 OSS 고지다.

이 텍스트들은 포함된 OSS 구성요소의 라이선스·저작권 고지와 대응 소스 제공 안내를 담고 있다. 본 SDK trial의 평가 기간·평가 용도, SDK 역공학/분석 금지, SDK 재배포 조건, SDK로 만든 출력물의 사용·상업적 이용 제한을 정하는 본 약관은 제공 패키지에서 찾지 못했다. OSS 구성요소에 대한 라이선스 고지는 전체 SDK의 사용 허가나 재배포 허가로 해석하지 않는다. 금지 조항이 없다고 추정하거나 사용 가능 여부를 판정하지 않는다.

**공식 사이트 https://sdk.hancom.com/ 의 약관 확인 필요(사용자 확인 사항).** 이 작업에서는 네트워크 접속을 하지 않았다. 약관 확인 전에는 trial 이용 조건, 배포, 산출물 이용 가능 범위를 미확정으로 둔다.

## 3. 공개 Python API 정리

아래 내용은 hwpsdk.py의 래퍼 시그니처와 샘플의 호출 순서로만 정리했다. 지원되는 형식 문자열, 인자 값의 완전한 의미, 각 native 함수의 내부 동작까지 보증하지 않는다.

래퍼 클래스는 Application, HwpObject, Window, Document, Pages, Action, ParameterSet, ParameterArray, CtrlCode, DocumentComparer, Commander, DocumentAnalyzer다. 보조 함수는 GetStringValue, GetStringArrayValue, GetStringChar16이다. 아래에는 문서 검증과 직접 관련된 메서드 및 라이선스 초기화 경로를 적었다.

### 로딩·초기화·라이선스 흐름

- hwpsdk.py는 SDK_BIN 환경 변수를 우선하고 없으면 hwpsdk.py가 놓인 폴더를 사용한다. Windows에서는 ctypes.CDLL(<경로>/SHDKFunctionExport.dll)을 연다. 소스에 Linux의 .so와 macOS의 .dylib 경로도 있지만, 이 trial ZIP에는 .so/.dylib가 없고 DLL만 있어 이 패키지는 Linux에서 실행할 수 없다. (Bin/hwpsdk.py:6-18)
- 샘플은 Python 포인터 크기에 따라 Bin64를 먼저 찾거나 32비트에서는 Bin을 찾는다. os.add_dll_directory, sys.path, 임시 PATH를 설정해 바인딩과 DLL을 로드한다. 두 폴더의 hwpsdk.py는 바이트 기준 동일하다. (HwpSDKSample.py:22-86)
- 호출 순서는 Application.SetLicensePath(path)(선택, Initialize 전) → Application.Initialize() → Application.GetHwpObject() → 필요하면 HwpObject.CreateHWindow(...) → Window.GetDocument() → 문서 작업 → 창 파기 → Application.Finalize()다. 샘플은 초기화 반환값이 양수이기를 기대하고 객체/창 포인터가 비었는지 확인한다. (hwpsdk.py:1036-1056, HwpSDKSample.py:126-150, 1691-1722)
- 샘플 주석은 라이선스 경로를 지정하지 않으면 엔진이 DLL 폴더에서 EXE 폴더 쪽 License를 자동 탐색한다고 설명한다. 바인딩은 SetLicensePath 외에 LicenseManager나 Sentinel RMS를 직접 호출하지 않는다. 패키지에 HncSentinelRMS.dll이라는 파일이 있다는 점만으로 RMS나 HancomLicenseManager.exe가 필수라고 판정할 수 없다.
- ProductConfig.xml은 Product/Version 구조이며, 읽은 XML에 라이선스 키·계정 필드는 없었다. 값은 기록하지 않았다. LicenseManager EXE는 실행하지 않고 PE 헤더/버전 리소스만 확인했다.

### 문서·텍스트·편집 API

| 목적 | 공개 래퍼 | 바인딩에서 확인한 입력/결과 |
|---|---|---|
| 문서 열기 | Document.Open(path, format=None, args=None) | ctypes.c_bool 반환으로 선언. 샘플은 format/args에 빈 문자열을 전달하고 실패 시 GetLastErrorCode()를 표시한다. |
| 저장·다른 형식 저장 | Document.Save(save_if_dirty=True), Document.SaveAs(path, format, args) | native 반환값을 전달한다. 샘플 Save As 메뉴는 확장자를 HWP/HWPX 문자열로 대응시킨다. |
| 형식/필터 확인 | Document.GetFilterList(flags) | 문자열 버퍼 래퍼를 사용한다. 지원 포맷 문자열 전체는 정적 바인딩에서 확인되지 않았다. |
| 텍스트 추출 | GetText(), GetTextFile(format, option), GetPageText(page, option), GetSelectText() | 문자열 또는 빈 버퍼일 때 None. GetTextFile/GetPageText 옵션의 세부 의미는 별도 API 문서가 필요하다. |
| 필드 텍스트 | GetFieldList(number=0, option=0), GetFieldText(fieldlist), PutFieldText(fieldlist, textlist) | 문자열 버퍼 래퍼 사용. 다중 항목은 \x02 구분자를 써서 Python 문자열 또는 리스트로 변환한다. 샘플은 필드 목록·내용 가져오기와 쓰기를 보여준다. |
| 선택·위치 | SelectText(spara, spos, epara, epos), GetPos(), SetPos(list, para, pos), GetSelectedPos() | GetPos()는 (list, para, pos) 세 정수 튜플, GetSelectedPos()는 시작/끝의 list·para·pos 여섯 정수 튜플을 반환한다. |
| 액션 편집 | Document.CreateAction(actionname) → Action.CreateParameterSet() → GetDefault(set) → Execute(set) | 샘플은 InsertText 액션의 Text, AllReplace 액션의 FindString/ReplaceString 등을 설정한다. Execute 반환을 무시하고 GetLastErrorCode()를 확인하는 샘플 사례가 있다. |
| 직접 편집 | Insert, InsertPicture, SetTextFile, SetCharShape/SetParaShape 등 | 편집·서식 호출 래퍼가 있다. 허용 값·단위는 샘플 또는 별도 API 문서에서 확인해야 한다. |

문자열 반환 helper는 길이를 먼저 질의하고 버퍼를 다시 받아 Python str로 만든다. 문자열 배열 helper는 \x02 기준으로 나눠 단일 값이면 str, 여러 값이면 list[str]를 반환한다. 반환값을 선언하지 않은 함수는 ctypes/native 원시 반환을 그대로 전달하므로, 성공 의미를 함수마다 확인해야 한다. (hwpsdk.py:24-44)

### 페이지·이미지·좌표·오류 API

- 쪽: Document.GetPageCount(), Document.GetPages() → Pages, Pages.GetPageCount(), Pages.GetPageRect(pageindex, rect), Pages.DrawPage(bitmap_buffer, pageindex, width, height).
- 이미지 출력: Document.CreatePageImage(path, pgno=0, resolution=96, depth=24, format=None, options=0), GetCtrlToPicture(path, format, fullsave), DocumentAnalyzer.CreateDocumentImage(docpath, imagefolderpath), CreateDocumentsImage(docfolderpath, imagefolderpath)가 있다. 형식 문자열과 페이지 번호 기준은 이 바인딩에 설명되어 있지 않다. DocumentAnalyzer.ProcessDla(dlaurl, ...)도 노출되지만 호출하지 않았다.
- 객체 위치: GetCtrlHorizontalOffset(pCtrl, relTo), GetCtrlVerticalOffset(pCtrl, relTo), CtrlCode.GetAnchorPos(type)가 있고, GetPageRect는 호출자가 넘기는 rect 인자를 native 함수에 그대로 전달한다. 좌표 단위·사각형 구조체 정의는 여기서 확인되지 않았다.
- 위치 설정의 대안: GetPosBySet()/SetPosBySet(pos)는 위치 정보를 ParameterSet으로 주고받는 래퍼다.
- 문서 비교 보조: DocumentComparer.Compare(srcpath, targetpath), GetDiffCount(), GetDiffItem(index), ExportDiffItems(exportpath), ExportReportFile(exportpath, format, nExportType)가 노출된다. 이 비교 결과의 판정 기준과 형식은 Windows 검증이 필요하다.
- 오류: Application.GetLastErrorCode()와 HwpObject.GetLastErrorCode()가 있다. 바인딩에 오류 코드 상수/표는 없다. 샘플은 초기화는 양수 반환을 기대하고, AllReplace는 마지막 오류 코드 0을 성공 사례로 사용한다. 이를 다른 API의 보편적 성공 규칙으로 확대하지 않는다.

**PDF는 미확인이다.** ExportPDF 전용 래퍼는 찾지 못했다. 일반 SaveAs(path, format, args)는 형식 인자를 받지만 샘플에서 확인한 형식은 HWP/HWPX뿐이다. PrintDocument()도 출력 경로 인자가 없다. PDF가 필요하면 Windows에서 GetFilterList(flags) 또는 공식 API 문서로 PDF 필터 지원을 먼저 확인한 뒤 별도 시험해야 한다.

### PE 메타데이터와 파일 목록

SDK ZIP에는 246개 일반 파일이 있으며, LicenseManager ZIP에는 EXE와 ProductConfig XML 두 파일이 있다. 전체 파일별 크기·SHA-256은 아래 로컬 격리 인벤토리에 기록했다. 입력 ZIP 자체의 SHA-256도 함께 적었다.

- SDK ZIP SHA-256: b2978cb2ecbdbdc75e204a9166bbf215e9c86b265e3c7e0d73627a66afcc0185
- LicenseManager ZIP SHA-256: 16938597a3fa6a8f5990f4e5c077949dad5162036e2e04ed3f01ce1f80bd0c25
- [SDK 전체 파일 목록·크기·SHA-256](../../spikes/work/hancom-sdk-trial/sdk-package-inventory.tsv)
- [LicenseManager 파일 목록·크기·SHA-256](../../spikes/work/hancom-sdk-trial/license-manager-inventory.tsv)

선별된 PE 메타데이터는 다음과 같다. 이는 헤더/버전 리소스만 읽은 결과이며, 파일 내용이나 내부 구현을 분석한 것이 아니다.

| 경로 | PE 아키텍처 | 버전 리소스 |
|---|---|---|
| Bin/shdk.dll, Bin/SHDKFunctionExport.dll | PE32 / Intel i386 | FileVersion 13.60.0.96 |
| Bin64/shdk.dll, Bin64/SHDKFunctionExport.dll | PE32+ / AMD64 | FileVersion 13.60.0.96 |
| Bin/HncSentinelRMS.dll | PE32 / Intel i386 | FileVersion 문자열 없음 |
| Bin64/HncSentinelRMS.dll | PE32+ / AMD64 | FileVersion 문자열 없음 |
| HancomLicenseManager.exe | PE32 / Intel i386 | FileVersion 1.0.1 |

두 hwpsdk.py 파일은 각각 48,445 bytes이며 SHA-256이 동일하다(3f6d3a6e96b31bf1d7a1d80653870db43c96dbaafdf426c30ece3c53eabee81d). 샘플은 63,365 bytes이며 SHA-256은 25619038c73b37b4a177397217c1d33570e3f5e360ade96e2ae1c03a5127c46b다. Notice 파일별 크기·해시는 전체 인벤토리를 참조한다.

검토 핵심 파일의 원본 아카이브 항목 크기·SHA-256:

| 항목 | bytes | SHA-256 |
|---|---:|---|
| Bin/shdk.dll | 23,101,464 | fabcee033fb84be9327f951c5cc549bb0c5c59fe23cc0ee09b63ea156cb69293 |
| Bin/SHDKFunctionExport.dll | 38,424 | 54f049c08d7b9bfb5811076f86f678ed1c2b2c0a01340314edf225c0746f334a |
| Bin/HncSentinelRMS.dll | 4,614,368 | 7e1e32c96283155f56215e9b561f1c77d314c7d190a3063727252a2828e743d7 |
| Bin64/shdk.dll | 30,206,488 | d7802bd2ef2e84c7831f31c3efcf99934cc694bfabc61e7304790913724cf5c2 |
| Bin64/SHDKFunctionExport.dll | 39,448 | 3f89e8c25bdfdec6533935a20aea54da2ad1221a7aa478014dbb46a0fed145cb |
| Bin64/HncSentinelRMS.dll | 5,558,496 | e517ffb6d654e5e33d3dc116acdaebb0fbc0e963fb304f326c87d964d04a9f97 |
| HancomLicenseManager.exe | 73,210,168 | ceae66b5b2d65050267d9c41a485a70a24eca4928906958b5e6204d727d4e990 |
| ProductConfig.xml | 986 | 0fb66fbe48f38f0888b222c6a743353dc797d15a91cbd46af82d1abf3996404c |

## 4. Windows 독립 엔진 검증 제안

아래는 아직 실행하지 않은 계획이다. 결과나 성능 향상 주장을 포함하지 않는다.

| 검증 | 필요한 API/입력 | 조건·산출 |
|---|---|---|
| HWPX 열기·쪽 수·텍스트 기준값 | 합성/시험용 엔진 출력 HWPX, Document.Open, GetLastErrorCode, GetPageCount, GetText, GetPageText | SDK 버전·Windows·비트수·폰트를 고정하고 입력 SHA-256을 기록한다. 열기 성공 여부, 전체/쪽별 텍스트와 쪽 수를 보존한다. |
| 쪽 사각형·페이지 이미지 비교 | GetPages, Pages.GetPageRect, Document.CreatePageImage 또는 Pages.DrawPage | 좌표 단위, page index 기준, 이미지 형식을 먼저 확인한다. 고정 DPI/비트수·폰트에서 페이지별 이미지를 만들고 엔진 렌더와 비교한다. API 호출의 정확한 입력·반환 및 시각 비교를 별도 기록한다. |
| SDK 재저장 왕복 | Document.SaveAs(path, "HWPX", args), 새 Document.Open, 위의 텍스트/쪽 조회 | 입력을 복사한 임시 파일에 저장하고 다시 연다. 원본과 재저장본의 페이지 수·텍스트·오류 코드를 비교하고, 입력·출력 hash를 남긴다. |
| 문서 차이 요약 | HwpObject.GetDocumentComparer, Compare, GetDiffCount, GetDiffItem | 동일한 합성 입력으로 비교 API가 내는 차이 항목과 한계를 기록한다. 차이 0을 렌더링 동일성으로 해석하지 않는다. |
| PDF 내보내기 | 우선 GetFilterList(flags) 또는 공식 문서에서 지원 필터 확인; 확인될 때만 SaveAs(..., "PDF", ...) 후보 시험 | 바인딩·샘플만으로 PDF 지원은 확인되지 않았다. 형식 지원과 사용 허가를 확인한 후 Windows에서 별도 시험한다. 지원되지 않으면 PDF 결과를 만들었다고 기록하지 않는다. |

실행 조건은 Windows, Python과 SDK DLL의 같은 비트수(Bin/Bin64), SDK에 필요한 DLL·리소스·폰트, 유효한 사용 권한/라이선스다. 샘플은 HWND를 만들어 한글 창을 포함하므로 GUI 샘플에는 Windows 데스크톱 세션이 필요하다. headless 사용 가능 여부는 바인딩만으로 단정하지 않는다. Linux는 현재 제공 패키지에 DLL만 있어 실행 대상이 아니다.

## 5. 클린룸 인계 규칙

인계가 필요해지면 docs/decisions/clean-room.md의 관찰 양식에 맞춰 검토된 동작 사실만 별도 명세로 전달한다.

**엔진 구현자에게 전달 가능**

- 합성 입력의 식별자와 SHA-256, 공개 API 호출명과 필요한 입력값의 범주, 관찰된 성공/오류, 쪽 수·텍스트·좌표·이미지 비교 결과, 재현 조건(Windows/SDK/비트수/폰트).
- 관찰마다 specId, evidenceType=observation, inputSha256, outputSha256, conditions, deliveredScope, excludedScope를 기록한 검토 완료 명세.
- 실제 입출력 대조로 얻은 동작 사실과 알려진 한계. 추정은 별도 표시하고 구현 규칙으로 승격하지 않는다.

**전달하지 않을 것**

- SDK ZIP과 DLL/EXE 원본 파일 자체(독점 바이너리 재배포).
- LicenseManager 설정 값, 라이선스 키·계정 정보, 개인 문서 본문, 관찰 담당자의 전체 대화/프롬프트/작업 폴더.
- 합성 시험의 전체 원시 산출물 대신, 검토된 동작 명세와 필요한 합성 입력·출력 hash만 전달한다.

이 문서는 출처 격리와 API 표면을 기록한 것이며 SDK 약관의 법률 판정이나 Windows 한컴 동작의 실측 결과가 아니다.

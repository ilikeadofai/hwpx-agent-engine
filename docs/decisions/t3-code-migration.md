# T3 Code nightly 이전 (2026-10-16)

이 프로젝트 작업은 Codex 데스크톱에서 T3 Code nightly(0.0.46-nightly.20261015.3176)로 옮겼다.

- T3 프로젝트 `Transformer 03 · HWPX 엔진 설계`: /home/mattc/Coding/transformer-03-multi-head-attention
- T3 프로젝트 `hwpx-agent-engine`: 이 저장소
- 설계·단계 0을 진행한 Codex 대화(01a132cd-f7f4-77a2-ab39-9abe6005de49)는 T3 스레드 `HWPX 엔진 설계·단계 0 (Codex 본체)`로 가져왔다. T3는 같은 Codex 세션을 이어서 실행하므로 전체 맥락이 유지된다. T3 화면에는 가져온 시점까지의 메시지 155개가 보인다.
- 같은 폴더의 하위 에이전트 대화 8개도 함께 가져왔다. 가져오기 화면을 띄우려고 만든 짧은 연결용 실행 1개는 보관 처리했다.

T3는 Codex 데스크톱 관리 폴더(~/Documents/Codex 아래)의 기록을 가져오지 않는다. 그래서 작업 폴더를 같은 파일시스템의 ~/Coding으로 옮기고 원래 위치에 심볼릭 링크를 남겼다. 기존 문서·메모의 절대 경로는 링크를 통해 그대로 열린다. 학교 연구 공유 폴더(…/referenced-chatgpt-conversation-this-is-an/outputs)와 이 저장소는 옮기지 않았다.

가져오기는 T3 자체 RPC(agentSessions.import)를 호출했다. 도구: ~/.local/share/t3-migrate/ (import_when_idle.mjs, t3_rpc.mjs). 작업에 쓴 임시 인증 세션은 모두 해제했다.

같은 Codex 세션을 Codex 데스크톱과 T3에서 동시에 이어 쓰지 않는다. 이후 작업은 T3에서 진행한다.


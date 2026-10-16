# 모델 역할 분담

개정: 2026-10-16 (T3 Code nightly로 옮긴 뒤 다시 정함). 이전 판의 원칙은 유지한다: 총괄과 Sonnet 5.5가 작업을 나누고, 복잡한 일은 Opus 5.5, 간단한 일은 Gemini 3.8 Flash, 단순 코드 작성은 DeepSeek 4.1 Flash가 맡는다. 바뀐 점은 총괄이 Codex의 GPT 6.1 Sol에서 T3의 Claude Opus 5.5로 넘어왔다는 것이다. 라우터·자동 분배는 만들지 않고 총괄이 작업마다 직접 배정한다.

## 쓸 수 있는 모델 (2026-10-16 T3 orchestrator_capabilities 기준)

| 경로 | 주요 모델 |
|---|---|
| claudeAgent (T3 기본) | claude-opus-5-5, claude-sonnet-5-5, claude-haiku-5-5, claude-fable-5-1 외 이전 세대 |
| codex | gpt-6.1-sol, gpt-6-astra, gpt-daybreak-blue-latest, google-antigravity/gemini-3.8-flash, opencode-go/deepseek-v4.1-flash, opencode-go/gemini-4-argon-contributor 등 |
| opencode | gemini-4-argon, deepseek-v4-pro, glm-5.3, kimi-k3, qwen3.8, grok-4.7 등 다수 |
| antigravity | gemini-3.8-flash (low/medium/high) |
| Claude Code 하위 에이전트 (opencodex 경유) | ocx-deepseek(~deepseek/deepseek-flash-latest), ocx-gemini-3-8-flash, ocx-claude-haiku-5-5 |

## 배정

| 모델 | 맡는 일 | 맡기지 않는 일 |
|---|---|---|
| **Claude Opus 5.5 (총괄, 이 스레드)** | 설계 판단, 단계 0 판정, 구현 계획 작성, 정규 상태·트랜잭션·원본 보존 핵심 코드, 하위 결과 검수와 최종 검증 | — |
| **Claude Sonnet 5.5** | 중간 난도 조사·구현(예: rhwp 출력 형식 조사), 설계·계획 일관성 검토, 코드 리뷰 | 단계 판정, 범위 변경 결정 |
| **GPT 6.1 Sol (codex)** | 다른 공급자 시각의 독립 검토: 구현 계획·핵심 코드의 반례와 빈틈 찾기 | 최종 결정, 본 구현 |
| **DeepSeek 4.1 Flash** | 인터페이스와 통과 시험이 정해진 단순 코드: 시험 보조 스크립트, 오류 코드 열거, 직렬화 구조체, CLI 인자 연결 | 원본 보존 XML 편집, 트랜잭션·이력, 격리 정책, 클린룸 자료 경계에 닿는 코드 |
| **Gemini 3.8 Flash** | 한국어 문서 표현·용어 점검, 짧은 요약, 단순 확인 | 코드와 설계 판단 |
| **Gemini 4 Argon** | 사용자에게 보일 긴 한국어 문서(안내서·사용 설명)의 문장 다듬기 | 기술 판단 |
| Claude Haiku 5.5 | 파일 목록·해시 대조 같은 기계적 확인이 많을 때 | 판단이 필요한 일 |

Claude Fable 5.1은 Opus로도 결론이 안 나는 고난도 검토가 생길 때만 쓰고, 기본 배정에는 넣지 않는다. 그 밖의 opencode 모델(GLM, Kimi, Qwen, Grok 등)은 배정하지 않는다.

## 규칙

- 하위 모델에는 대상 파일·함수 시그니처·통과해야 할 시험을 함께 준다. 결과는 총괄이나 Sonnet이 읽고 시험을 직접 다시 돌린 뒤 받아들인다.
- 외부 공급자(DeepSeek, Gemini, GPT)로는 사용자 문서·개인 자료·실명이 든 문서 본문을 보내지 않고, 명세와 공개·합성 자료만 보낸다.
- 응답한 실제 모델명과 작업 범위를 작업 기록에 남긴다.
## 호출 경로 (2026-10-16 확인)

| 모델 | 경로 | 확인 |
|---|---|---|
| Sonnet 5.5 | Claude Code 하위 에이전트(model=sonnet) 또는 T3 delegate_task(claudeAgent) | 사용 중 |
| DeepSeek 4.1 Flash | **T3 delegate_task, providerInstanceId=opencode, model=opencode/deepseek-v4.1-flash-free** | S4 스크립트 작성에 사용, 응답 정상 |
| GPT 6.1 Sol | T3 delegate_task, providerInstanceId=codex, model=gpt-6.1-sol | — |
| Gemini 4 Argon | T3 delegate_task, providerInstanceId=opencode, model=opencode/gemini-4-argon | — |
| Gemini 3.8 Flash | T3 delegate_task, providerInstanceId=antigravity | — |

- 다른 공급자 모델은 T3의 opencode·codex·antigravity 공급자로 부른다. OpenCode 무료 등급은 OpenCode 앱 안에서만 허용되므로, T3의 opencode 공급자(앱을 직접 실행)로는 되지만 opencodex 프록시로 직접 부르면 403이다. 프록시의 opencode-go 경로도 작업공간 지역 설정 때문에 400으로 거절됐다.
- **Claude Code의 ocx-* 하위 에이전트 유형은 쓰지 않는다.** 2026-10-16에 ocx-deepseek 유형으로 맡긴 작업이 실제로는 Haiku 5.5로 실행됐다(프록시 로그에 DeepSeek 호출 없음). 그 결과물은 채택하지 않고 spikes/work/discarded_haiku/에 두었다.
- OpenRouter 경로(spikes/tools/ask_model.py)는 OpenCode 경로가 안 될 때만 쓴다.

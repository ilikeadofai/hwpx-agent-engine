# 모델 역할 분담

개정: 2026-10-16(사용자 지정 사용 계획 반영). 각 요금제의 사용량 한도에 빨리 닿지 않도록 사용자가 모델별 역할을 정했다. 이 표가 현재 기준이며, 아래 "이전 기록"은 참고용이다.

## 현재 배정 (사용자 지정)

| 계층 | 모델 | 맡는 일 | 하지 않는 일 |
|---|---|---|---|
| 총괄 | **Claude Opus 5.5** | 전체 관리: 단계·범위 판단, 작업 분해와 배정, 결과 수용 결정, 커밋 | 매우 복잡한 작업이 아니면 직접 구현하지 않는다. 멀티모달 작업 |
| 총괄 보조·검증 | **GPT 6.1 Sol (codex)** | 오케스트레이션 보조, 계획·결과의 독립 검증과 검토 | 매우 복잡한 작업이 아니면 직접 구현하지 않는다. 멀티모달 작업 |
| 구현 계획 수행 | **Claude Sonnet 5.5** (주력) | 총괄이 넘긴 구현 계획의 실행: 엔진 기능 구현, 시험, 중간 난도 조사 | 범위 변경 결정 |
| 구현 계획 수행 | **Gemini 4 Argon** (opencode-go) | 사용자가 실제로 경험하는 부분: CLI·MCP 도구 설명과 응답 문구, 오류 메시지, 사용 안내서, 미리보기 산출물의 사용자 표시 | 핵심 보존·트랜잭션 코드 |
| 세부·단순 작업 | **DeepSeek V4.1 Flash** (opencode-go) | 정해진 시그니처와 시험이 있는 코드, 보조 스크립트, 반복 작업. Gemini 3.8 Flash와 나눠 맡는다 | 원본 보존 XML 편집, 트랜잭션·이력, 격리 정책의 설계 |
| 세부·단순 작업 | **Gemini 3.8 Flash (High)** (antigravity) | 비전 등 멀티모달 작업(미리보기 이미지 판독, 렌더링 비교, 스크린샷 확인). 그 밖의 단순 작업은 DeepSeek와 나눠 맡는다 | 설계 판단 |
| 세부·단순 작업 | **Claude Haiku 5.5** | 에이전틱 작업: 여러 명령을 돌리며 확인하는 일(빌드·시험 반복, 파일 대조, 환경 점검) | 판단이 필요한 설계 |
| 세부·단순 작업 | **Muse Spark 1.3** (opencode-go) | 빠른 처리가 필요한 짧은 작업 | 긴 구현 |

호출하지 않는 모델: Claude Fable, GPT-6 Astra. OpenRouter 경로는 쓰지 않는다. OpenCode 모델은 opencode-go 모델 id로 부른다.

## 호출 경로

| 모델 | 경로 |
|---|---|
| Sonnet 5.5, Haiku 5.5 | T3 delegate_task(providerInstanceId=claudeAgent) 또는 Claude Code 하위 에이전트(model=sonnet/haiku) |
| GPT 6.1 Sol | T3 delegate_task(providerInstanceId=codex, model=gpt-6.1-sol) |
| Gemini 4 Argon | T3 delegate_task(providerInstanceId=opencode, model=opencode-go/gemini-4-argon-contributor) |
| DeepSeek V4.1 Flash | T3 delegate_task(providerInstanceId=opencode, model=opencode-go/deepseek-v4.1-flash) |
| Muse Spark 1.3 | T3 delegate_task(providerInstanceId=opencode, model=opencode-go/muse-spark-1.3-contributor) |
| Gemini 3.8 Flash (High) | T3 delegate_task(providerInstanceId=antigravity, model=gemini-3.8-flash-high) |

- Claude Code의 ocx-* 하위 에이전트 유형은 쓰지 않는다(2026-10-16에 지정 모델 대신 Haiku로 실행되거나 404로 실패).
- T3 delegate_task가 거절되면(parent_not_active 등) 해당 작업은 경로가 살아날 때까지 미루고, 총괄이 대신 구현하지 않는다. 기다릴 수 없는 작업만 Claude 하위 에이전트(Sonnet·Haiku)로 돌리고 작업 기록에 남긴다.

## 규칙

- 하위 모델에는 대상 파일·소유 범위·함수 시그니처·통과해야 할 시험을 함께 준다. 같은 파일을 두 작업이 동시에 소유하지 않는다.
- 결과는 검증 담당(GPT 6.1 Sol 또는 Haiku의 시험 재실행)을 거친 뒤 총괄이 받아들인다.
- 외부 공급자로는 사용자 문서·개인 자료·실명이 든 문서 본문을 보내지 않고, 명세와 공개·합성 자료만 보낸다.
- 응답한 실제 모델명과 작업 범위를 작업 기록(.superpowers/sdd/…/progress.md)에 남긴다.

## 이전 기록 (요약)

- 2026-10-16 오전: Opus 총괄, Sonnet 중간 구현, GPT 6.1 Sol 독립 검토, DeepSeek 단순 코드, Gemini 3.8 Flash 한국어 점검, Gemini 4 Argon 긴 문서.
- 2026-10-16 오후: 총괄이 일시적으로 Codex GPT 6.1 Sol로 넘어가 OpenCode·Codex 모델에 파일 범위를 나눠 위임했다(단계 1 완료). 이 기간에 Astra도 일부 쓰였으나 이후 사용자 지시로 중단했다.
- ocx-deepseek 유형으로 맡긴 작업이 Haiku로 실행된 사례의 결과물은 채택하지 않고 spikes/work/discarded_haiku/에 두었다.

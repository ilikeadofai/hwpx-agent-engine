# 모델 역할 분담

개정: 2026-10-17(사용자 지정: Sol 오케스트레이터·Opus 계획 전용, 사용 한도 균형 배정). 각 요금제의 사용량 한도에 빨리 닿지 않도록 사용자가 모델별 역할을 정했다. 이 표가 현재 기준이며, 아래 "이전 기록"은 참고용이다.

## 현재 배정 (사용자 지정)

| 계층 | 모델 | 맡는 일 | 하지 않는 일 |
|---|---|---|---|
| 오케스트레이터 | **GPT 6.1 Sol (codex)** | 전체 진행 관리, 작업 분해·모델 배정, 결과 수용, 통합·구현·커밋. Codex·Claude 한도를 확인하며 배정 조절 | 멀티모달 작업 자제. 자신이 구현한 결과의 독립 최종 검증은 다른 담당에게 배정 |
| 계획 전용 | **Claude Opus 5.5** | 복잡한 설계·계획 수립과 계획 검토가 필요할 때만 호출 | 구현·작업 배정·오케스트레이션·반복 검증·커밋 담당으로 호출하지 않는다 |
| 구현 | **GPT 6.1 Sol** (codex) | 구현 비중이 큰 작업: 엔진 기능 구현, 결함 수정, 원인 분석 뒤 수정, 시험 추가 | 범위 변경 결정 |
| 구현 계획 수행 | **Claude Sonnet 5.5** | 구현 비중이 작은 작업: 보조 스크립트, 중간 난도 조사, 문서와 묶은 정리, Sol 구현의 독립 검증 | 범위 변경 결정 |
| 구현 계획 수행 | **Gemini 4 Argon** (opencode-go) | 사용자가 실제로 경험하는 부분: CLI·MCP 도구 설명과 응답 문구, 오류 메시지, 사용 안내서, 미리보기 산출물의 사용자 표시 | 핵심 보존·트랜잭션 코드 |
| 세부·단순 작업 | **DeepSeek V4.1 Flash** (opencode-go) | 정해진 시그니처와 시험이 있는 코드, 보조 스크립트, 반복 작업. Gemini 3.8 Flash와 나눠 맡는다 | 원본 보존 XML 편집, 트랜잭션·이력, 격리 정책의 설계 |
| 세부·단순 작업 | **Gemini 3.8 Flash (High)** (antigravity) | 비전 등 멀티모달 작업(미리보기 이미지 판독, 렌더링 비교, 스크린샷 확인). 그 밖의 단순 작업은 DeepSeek와 나눠 맡는다 | 설계 판단 |
| 세부·단순 작업 | **GPT 6 Luna (reasoning max)** 우선, **Claude Haiku 5.5** 보조 | 에이전틱 작업: 여러 명령을 돌리며 확인하는 일(빌드·시험 반복, 파일 대조, 환경 점검, 결과 분석). 둘을 비슷한 용도로 쓰되 Luna를 더 자주 배정 | 판단이 필요한 설계 |
| 세부·단순 작업 | **Muse Spark 1.3** (opencode-go) | 빠른 처리가 필요한 짧은 작업 | 긴 구현 |

호출하지 않는 모델: Claude Fable, GPT-6 Astra. OpenRouter 경로는 쓰지 않는다. OpenCode 모델은 opencode-go 모델 id로 부른다.

## 사용 한도에 따른 운영

- Codex는 app-server `account/rateLimits/read`, Claude는 현재 로그인 계정의 읽기 전용 사용량 응답에서 5시간·주간 사용률과 초기화 시각을 확인한다. 모델 요청으로 한도를 시험하지 않는다. 계정 식별자·인증값·원시 인증 응답은 기록하지 않는다.
- 큰 작업을 새로 배정하기 전과 긴 작업이 끝난 뒤 다시 확인한다. 진행 중에는 약 10~15분 간격을 기본으로 하고, 한도 도달·초기화가 가까우면 다음 배정 전에 재조회한다. 별도의 무기한 감시 프로세스나 반복 모델 호출을 만들지 않는다.
- 고정된 작업 수 50:50 대신 두 공급자의 5시간·주간 잔량, 초기화까지 남은 시간, 최근 작업의 사용량 증가를 함께 보고 분담한다. 한쪽에 긴 작업을 몰지 않으며 최종 통합·검증 여유분을 남긴다. Sol이 계속 총괄하고, Claude 여유가 크면 Sonnet 구현·독립 검토를 우선한다.
- 사용률 80% 이상이면 새 장기 작업을 줄이고, 90% 이상이거나 한도 도달이면 새 병렬 작업을 보류한다. 다른 공급자의 잔량과 기존 OpenCode/비전 역할을 이용한다. 두 공급자가 모두 부족하면 범위가 작은 준비·로컬 검사부터 수행한다.
- Claude가 5시간 한도에 걸렸으면 초기화 시각 전에는 새 Claude 작업을 시작하지 않는다. 시각 경과만으로 재개하지 않고 사용량 감소/가용 상태를 다시 확인한 뒤 Sonnet·Haiku 사용을 재개한다.
- Haiku 사용 불가 시 기존 지시대로 GPT 6 Luna를 사용한다. Opus는 계획 필요성이 있을 때만 사용한다. 나머지 모델 역할 및 DeepSeek의 `opencode-go` 경로는 유지한다. 유료 크레딧·한도 초기화권을 자동으로 소비하지 않는다.
- 재조회 도구: `python3 scripts/check_provider_usage.py`. 공개 가능한 사용률·시각만 출력하며 실제 수치는 추적하지 않는 `spikes/work/provider-usage/`에 저장한다. 정상적인 null 선택 항목은 무시하며, 읽을 수 없는 필수 사용률이 남아 있으면 다른 창의 정보는 보존하되 `capacityState`는 `unknown`으로 둔다. 이 도구의 `available`과 종료 코드 0은 **메타데이터를 읽었다**는 뜻일 뿐, 배정(inference) 승인이나 잔량 보장이 아니다. 배정 판단은 공급자별 `capacityState`(`headroom`/`at_limit`/`unknown`)와 창별 `usedPercent`·`resetsAt`으로 한다. `at_limit`은 유효한 창이 100% 이상이거나 명시적 차단 플래그(`rateLimitReachedType`, `spendControlReached`, `ordinaryUsageAllowed=false`)일 때이고, `unknown`은 유효한 창을 얻지 못한 불완전·비정상 응답이거나 조회 오류일 때다. `capacityState`는 메타데이터 기반 신호일 뿐 모델 요청 수락을 보장하지 않는다. 계정 식별자·토큰·원시 응답은 출력하지 않는다.

## 호출 경로

사용자 추가 지시: Haiku가 사용량 제한 등으로 실행되지 않으면 `providerInstanceId=codex`, `model=gpt-6-luna`로 같은 작업을 넘긴다. Astra로 자동 대체하지 않는다.

| 모델 | 경로 |
|---|---|
| Sonnet 5.5, Haiku 5.5 | T3 delegate_task(providerInstanceId=claudeAgent) 또는 Claude Code 하위 에이전트(model=sonnet/haiku) |
| GPT 6.1 Sol | T3 delegate_task(providerInstanceId=codex, model=gpt-6.1-sol) |
| GPT 6 Luna | T3 delegate_task(providerInstanceId=codex, model=gpt-6-luna, options.reasoningEffort=max) |
| Gemini 4 Argon | T3 delegate_task(providerInstanceId=opencode, model=opencode-go/gemini-4-argon-contributor) |
| DeepSeek V4.1 Flash | T3 delegate_task(providerInstanceId=opencode, model=opencode-go/deepseek-v4.1-flash) |
| Muse Spark 1.3 | T3 delegate_task(providerInstanceId=opencode, model=opencode-go/muse-spark-1.3-contributor) |
| Gemini 3.8 Flash (High) | T3 delegate_task(providerInstanceId=antigravity, model=gemini-3.8-flash-high) |

- Claude Code의 ocx-* 하위 에이전트 유형은 쓰지 않는다(2026-10-16에 지정 모델 대신 Haiku로 실행되거나 404로 실패).
- T3 delegate_task가 거절되면(parent_not_active 등) 해당 작업은 경로가 살아날 때까지 미루고, 총괄이 대신 구현하지 않는다. 기다릴 수 없는 작업만 Claude 하위 에이전트(Sonnet·Haiku)로 돌리고 작업 기록에 남긴다.

## 규칙

- 하위 모델에는 대상 파일·소유 범위·함수 시그니처·통과해야 할 시험을 함께 준다. 같은 파일을 두 작업이 동시에 소유하지 않는다.
- 결과는 구현자와 다른 검증 담당(Sonnet 구현은 GPT 6.1 Sol, GPT 6.1 Sol 구현은 Sonnet 또는 Haiku(사용 불가 시 GPT 6 Luna)의 시험 재실행)을 거친 뒤 총괄이 받아들인다.
- 외부 공급자로는 사용자 문서·개인 자료·실명이 든 문서 본문을 보내지 않고, 명세와 공개·합성 자료만 보낸다.
- 응답한 실제 모델명과 작업 범위를 작업 기록(.superpowers/sdd/…/progress.md)에 남긴다.

## 이전 기록 (요약)

- 2026-10-16 오전: Opus 총괄, Sonnet 중간 구현, GPT 6.1 Sol 독립 검토, DeepSeek 단순 코드, Gemini 3.8 Flash 한국어 점검, Gemini 4 Argon 긴 문서.
- 2026-10-16 오후: 총괄이 일시적으로 Codex GPT 6.1 Sol로 넘어가 OpenCode·Codex 모델에 파일 범위를 나눠 위임했다(단계 1 완료). 이 기간에 Astra도 일부 쓰였으나 이후 사용자 지시로 중단했다.
- ocx-deepseek 유형으로 맡긴 작업이 Haiku로 실행된 사례의 결과물은 채택하지 않고 spikes/work/discarded_haiku/에 두었다.

- 2026-10-17: 사용자 지시로 GPT 6.1 Sol을 상시 오케스트레이터로 지정하고 Opus 5.5는 계획 전용으로 제한했다. Claude 5시간 한도 해제 확인 후 재개하며 Codex·Claude 사용률을 함께 보고 배정한다. 이전 총괄 배정은 현재 지시를 대체하지 않는다.
- 2026-10-17 저녁: 사용자 지시로 Sonnet 몫 중 구현 비중이 큰 작업을 GPT 6.1 Sol로 옮기고, 에이전틱 작업은 GPT 6 Luna(reasoning max)를 Haiku보다 자주 배정한다. 이 세션의 진행 관리는 Opus가 맡되 직접 작업은 줄인다.

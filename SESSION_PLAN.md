## Session Plan

Day 3 (2026-03-20 16:00) — 팀 워크플로우: 데스크-기자 협업 시스템

### Self-Assessment Summary

빌드·테스트 모두 통과 (67 tests, 0 failures). 50개+ 커맨드로 개인 기자의 취재→출고→아카이브 파이프라인은 완성됐다. 커뮤니티 이슈 없음.

현재 파이프라인: 취재(clip·news·sources·alert) → 리서치(research+API) → 트렌드분석(trend) → 팩트체크(factcheck) → 취재현장(interview·compare·timeline) → 기사작성(article+templates) → 다듬기(translate·headline·rewrite·summary) → 편집(checklist·proofread·stats·quote) → 법적점검(legal) → 마감(draft·deadline·embargo·export) → 브리핑(briefing) → 아카이브(archive) → 후속추적(follow) → 데이터분석(data)

**전략적 분석:**
리서치 결과, 한국 뉴스룸의 가장 큰 마찰은 개인 기자 도구가 아닌 **팀 단위 협업**에서 발생한다:
1. **데스크-기자 업무 지시 추적** — Slack/메일로 흩어진 업무 지시, "이거 맡겼잖아?" 추적 불가
2. **속보 취재 중복** — "누가 뭘 취재 중인지" 모르고 같은 건에 복수 기자 투입되는 낭비
3. **공동취재 메모 산재** — 합동 취재 시 메모·클립이 개인별로 분산, 합치는 데 시간 소모

경쟁 도구(빅카인즈 AI, 로봇저널리즘, Superdesk)는 모두 작성·분석에 집중하고 있어 **팀 협업 레이어**는 빈 공간이다. 이번 세션에서 여기를 선점한다.

### Task 1: `/desk` — 데스크-기자 업무 지시 큐
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 데스크와 기자 간 업무 지시·피드백을 구조화한 태스크 큐 시스템. AI 호출 없이 로컬 동작.
- `assign <기자> <내용> [--deadline HH:MM]` — 데스크가 기자에게 업무 지시
- `list [--reporter 기자명]` — 현재 업무 목록 (마감순 정렬, 상태 색상 코딩)
- `done <번호>` — 업무 완료 처리
- `feedback <번호> <내용>` — 데스크 피드백 추가
- `pitch <제목> <내용>` — 기자가 기사 아이디어 제안
- .journalist/desk/assignments.json에 저장
- 테스트 먼저
Issue: none

### Task 2: `/collaborate` — 공동취재 메모 공유
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 복수 기자가 같은 취재건에 메모·클립·취재 노트를 공유하는 시스템. AI 호출 없이 로컬 동작.
- `start <프로젝트명> [--reporters 기자1,기자2]` — 공동취재 프로젝트 생성
- `note <프로젝트명> <내용> [--reporter 기자명]` — 메모 추가
- `list` — 활성 프로젝트 목록
- `view <프로젝트명>` — 프로젝트 전체 메모·클립 조회 (시간순)
- `close <프로젝트명>` — 프로젝트 종료
- .journalist/collaborate/에 프로젝트별 JSON 파일로 저장
- 테스트 먼저
Issue: none

### Task 3: `/coverage` — 속보 취재 중복 방지 트래커
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 속보 상황에서 "누가 뭘 취재 중인지" 추적하는 실시간 트래커. AI 호출 없이 로컬 동작.
- `claim <주제> [--reporter 기자명] [--until HH:MM]` — 취재 영역 선점 등록
- `list` — 현재 취재 중인 건 목록 (만료 시간 색상 코딩)
- `release <번호>` — 취재 영역 해제
- `check <키워드>` — 해당 주제가 이미 취재 중인지 확인
- .journalist/coverage.json에 저장
- 만료 시간 경과 시 자동 비활성 표시
- 테스트 먼저
Issue: none

### Task 4: journal entry
Files: `JOURNAL.md`
Description: 세션 결과 기록 — 팀 워크플로우 진입의 전략적 의미, 구현 결과, 다음 방향.
Issue: none

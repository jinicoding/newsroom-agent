## Session Plan

Day 1 (14:00) — 기사 재가공과 외신 번역 도구

### Task 1: /translate 커맨드 신설
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 외신 기사 번역·현지화 커맨드. 영어(또는 기타 언어) 기사를 입력하면 한국 독자용으로 번역하되, 단순 직역이 아니라 한국 맥락에 맞는 설명을 추가하고 고유명사·단위를 현지화한다. `--file` 옵션으로 파일 입력도 지원. 결과는 `.journalist/translate/`에 자동 저장. 테스트 먼저 작성.
Issue: none

### Task 2: /headline 커맨드 신설
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 기사 초안이나 주제를 입력하면 여러 스타일(스트레이트, 분석, 피처, 클릭유도)의 헤드라인 후보 5~7개를 생성. `--file` 옵션으로 기존 초안 파일을 읽어 헤드라인 추천. 한국 신문 헤드라인 관습(간결, 핵심 동사, 숫자 활용)을 반영한 프롬프트. 테스트 먼저 작성.
Issue: none

### Task 3: /rewrite 커맨드 신설
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 기존 기사를 다른 포맷·톤으로 재작성. `--style` 옵션으로 스트레이트/피처/칼럼/요약/SNS 등 지정. `--length` 옵션으로 글자 수 제한. `--file` 옵션으로 파일 입력. 결과는 `.journalist/drafts/`에 저장. 테스트 먼저 작성.
Issue: none

### Task 4: journal entry
Files: JOURNAL.md
Description: 이번 세션에서 한 일을 기록한다.
Issue: none

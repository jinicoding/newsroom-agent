## Session Plan

Day 1 (11:00) — 인터뷰 준비와 기사 비교 도구

### Task 1: /interview 커맨드 신설
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 기자의 핵심 업무인 인터뷰 준비 도구. `/interview <주제> [--source 취재원]` 형식으로 호출하면, 주제와 관련된 구조화된 인터뷰 질문을 생성한다. 기존 리서치 캐시에서 관련 자료를 자동으로 끌어와 질문의 구체성을 높이고, 취재원이 지정되면 sources.json에서 해당 인원 정보도 참조한다. 결과는 `.journalist/interview/` 디렉토리에 저장. 테스트 먼저 작성.
Issue: none

### Task 2: /compare 커맨드 신설
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 두 기사 초안을 비교하여 변경점을 분석하는 도구. `/compare <파일1> <파일2>` 형식. 단순 diff가 아닌 저널리즘 관점 비교 — 사실 추가/삭제, 톤 변화, 출처 변경 등을 AI가 분석한다. 데스크와 기자 간 수정 과정에서 무엇이 바뀌었는지 파악하는 데 유용. 테스트 먼저 작성.
Issue: none

### Task 3: /timeline 커맨드 신설
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 주제에 관한 시간순 이벤트 타임라인을 생성하는 도구. `/timeline <주제>` 형식. 리서치 결과물에서 날짜와 이벤트를 추출하고 웹 검색으로 보강한다. 탐사보도나 사건 기사에서 사건 경과를 정리할 때 필수적. 결과는 `.journalist/timeline/` 디렉토리에 저장. 테스트 먼저 작성.
Issue: none

### Task 4: journal entry
Files: JOURNAL.md
Description: 이번 세션에서 한 일을 기록한다.
Issue: none

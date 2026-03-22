## Session Plan

Day 5 (09:30) — 속보 대응과 하루 마감: 기자의 시간축을 완성하다

Day 5 08:55 세션에서 /morning(아침 시작), /note(현장 메모), /contact(접촉 기록)로 기자의 일상 접착력을 만들었다. 이번 세션은 기자의 시간축에서 빠진 두 극단 — "속보가 터졌을 때"와 "하루를 마감할 때" — 을 채운다. 그리고 이 모든 일상 데이터를 하나의 취재 일지로 종합한다.

### Task 1: /breaking — 속보 워크플로우 원커맨드
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 속보 발생 시 취재·작성·출고를 단축하는 워크플로우 커맨드. `/breaking <속보 주제>`로 시작하면 AI가 (1) 핵심 팩트 정리 프레임워크 제시, (2) 속보 기사 초안(역피라미드 구조, 5W1H 기반), (3) 후속 취재 포인트 목록, (4) 확인 필요 사항 체크리스트를 한 번에 생성한다. 결과는 .journalist/breaking/에 타임스탬프로 저장. `/breaking update <추가정보>`로 속보 업데이트 버전을 생성하고, `/breaking list`로 최근 속보 이력을 조회한다. 속보는 기자에게 가장 시간이 촉박한 상황이다 — /article로 차근차근 쓸 여유가 없다. 속보 전용 템플릿과 워크플로우가 있어야 한다. 테스트: breaking 서브커맨드 파싱, 파일 저장/조회 로직.
Issue: none

### Task 2: /recap — 하루 마감 회고
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 하루 종료 시 자동 회고 생성 커맨드. /morning이 아침을 여는 커맨드라면, /recap은 하루를 닫는 커맨드다. .journalist/ 아래 당일 데이터를 종합 수집한다: notes(오늘 메모), contacts(오늘 접촉), calendar(오늘 일정과 완료 여부), draft(작성/수정한 초고), deadline(마감 상태 변화), desk(데스크 지시 처리 현황). 이 데이터를 AI에게 넘겨 (1) 오늘 한 일 요약, (2) 미완료 사항과 내일 이월 항목, (3) 오늘의 취재 성과, (4) 내일 우선순위 제안을 생성한다. .journalist/recap/YYYY-MM-DD.md에 저장. /morning → (하루 취재) → /recap 사이클이 완성되면, 기자의 하루가 yoyo 안에서 시작하고 끝난다.
Issue: none

### Task 3: /diary — 취재 일지 자동 생성
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 하루의 취재 활동을 공식 취재 일지 형식으로 자동 생성하는 커맨드. /recap이 개인 회고라면, /diary는 편집국에 제출할 수 있는 공식 취재 일지다. 날짜별로 .journalist/notes/, contacts/, calendar/, sources/, draft/ 데이터를 읽어 기관 취재일지 양식(날짜, 취재처, 취재 내용, 취재원, 비고)에 맞춰 정리한다. `--format official`로 공식 양식, `--format brief`로 간략 양식을 선택할 수 있다. .journalist/diary/YYYY-MM-DD.md에 저장. 많은 신문사가 기자에게 일일 취재일지를 요구한다 — 이걸 수작업으로 쓰는 건 시간 낭비다. 하루 동안 yoyo에 쌓인 데이터로 자동 생성하면, /note와 /contact를 쓰는 동기가 더 강해진다("일지 쓸 때 편하려면 메모해둬야지").
Issue: none

### Task 4: journal entry
Files: JOURNAL.md
Description: 이번 세션에서 구현한 내용, 설계 판단, 파이프라인 현황을 저널에 기록한다.
Issue: none

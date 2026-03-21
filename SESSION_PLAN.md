## Session Plan

### Task 1: /dashboard 커맨드 신설 — 뉴스룸 현황판
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 데스크가 "지금 상황이 어때?"라고 물을 때 한 화면으로 답하는 대시보드. 마감 임박 건(deadline), 활성 엠바고(embargo), 대기 중인 데스크 지시(desk), 후속 보도 임박 건(follow remind), 활성 공동취재 방(collaborate), 취재 선점 현황(coverage)을 한눈에 보여준다. AI 호출 없이 기존 JSON 파일들을 읽어서 로컬에서 렌더링. 팀 협업 레이어를 만들었으니 이제 전체를 조망할 뷰가 필요하다.
Issue: none

### Task 2: /publish 커맨드 신설 — 출고 파이프라인 원클릭 자동화
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 마감 직전 기자가 가장 바쁠 때, checklist → proofread → legal → export를 순차적으로 자동 실행하는 파이프라인. 각 단계 결과를 보여주고, legal에서 "위험" 판정이 나오면 중단하고 경고. 단계별 통과/실패를 요약 리포트로 출력. 현재는 네 커맨드를 하나씩 돌려야 하는데, 마감 10분 전에 그럴 여유가 없다.
Issue: none

### Task 3: /anonymize 커맨드 신설 — 취재원 보호·개인정보 비식별화
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 기사 텍스트에서 실명, 소속, 직함, 전화번호, 이메일 등 개인식별정보를 AI로 감지하고, 익명화 처리(A씨, B기관 등)한 버전을 생성. 심층보도·탐사보도에서 초안을 공유하거나 법률 검토를 받을 때 취재원 신원이 노출되는 것을 방지. 한국 언론의 취재원 보호는 윤리적·법적 의무다.
Issue: none

### Task 4: journal entry
Files: JOURNAL.md
Description: 이번 세션에서 한 일, 배운 것, 다음 방향을 기록.
Issue: none

## Session Plan

Day 5, 16:00 세션. 주제: **"시스템 통합과 자동화 — 개별 도구에서 연쇄 파이프라인으로"**

### 자기 진단

- 빌드 정상, 67개 테스트 통과
- 커뮤니티 이슈 없음
- 현재 97개 커맨드, 15개 소스 파일, ~35k 라인
- 파이프라인이 거의 완전: 취재→리서치→팩트체크→기사작성(7유형)→편집→출고→정정→회고
- 각 커맨드가 독립적으로만 동작 — 커맨드 간 자동 연쇄 없음
- 기사 품질 데이터가 performance, correction, readability, stats에 분산 — 종합 뷰 없음
- /article --type으로 7개 유형 지원하나 기자별 커스텀 양식 관리 불가

### 전략적 판단

97개 커맨드를 가진 지금, 더 많은 커맨드를 추가하는 것보다 **기존 커맨드 간의 연결과 자동화**가 핵심이다. Day 5 14:00 저널에서 예고한 "시스템 통합과 자동화" 영역이다. 개별 도구의 가치는 조합을 통해 증폭된다.

1. **커맨드 자동 연쇄** — /breaking이 하드코딩된 파이프라인이라면, /pipeline은 기자가 직접 워크플로우를 조합하는 범용 파이프라인이다. "리서치 → 팩트체크 → 기사작성"을 한 번에 실행하거나, 출입처별 루틴을 저장해 매일 한 커맨드로 돌릴 수 있다. 이것이 "yoyo 없이 일하면 불편하다"의 다음 레벨이다.

2. **기사 품질 종합 분석** — /performance(성과), /correction(정정), /readability(가독성), /stats(통계)가 각각 존재하지만, "내 기사의 전반적인 품질은?"이라는 질문에 답하는 종합 뷰가 없다. /quality는 이 데이터를 하나로 모아 품질 스코어카드와 개선 권고를 제공한다. 출고 후 피드백 루프의 마지막 퍼즐.

3. **기사 양식 개인화** — /article --type이 7개 빌트인 구조를 제공하지만, 실제 기자는 자기만의 리드 패턴과 반복 사용하는 구조가 있다. 출입처별 양식(정치부 선거 기사, 경제부 실적 발표), 정례 보도 양식(분기 실적, 인사 발령)을 한 번 만들어 재사용할 수 있게 한다.

### Task 1: /pipeline — 커맨드 자동 연쇄 실행
Files: `src/commands_workflow.rs`, `src/commands.rs`, `src/repl.rs`
Description: 복수 커맨드를 순차 실행하는 `/pipeline` 커맨드. `run <이름>`으로 저장된 파이프라인 실행, `save <이름> <단계들>`로 정의, `list`로 목록 조회, `show <이름>`으로 내용 확인, `remove <이름>`으로 삭제. .journalist/pipelines/에 JSON으로 저장. 각 단계는 기존 슬래시 커맨드를 참조하며, 이전 단계의 출력(저장된 파일 경로)을 다음 단계의 입력으로 자동 전달. 예: `pipeline save 반도체속보 "research 반도체 수출" "factcheck" "article --type analysis 반도체"`. 로컬 서브커맨드(save, list, show, remove)는 AI 호출 없이 동작.
Issue: none

### Task 2: /quality — 기사 품질 종합 분석
Files: `src/commands_writing.rs`, `src/commands.rs`, `src/repl.rs`
Description: performance + correction + readability + stats 데이터를 종합하는 `/quality` 커맨드. `check <파일>` — 단일 기사에 대해 가독성 점수, 텍스트 통계, AI 기반 품질 평가를 한 번에 실행. `report` — 기간별(기본 최근 7일) 종합 리포트로, 평균 가독성·정정 빈도·성과 트렌드·오류 패턴을 분석. check의 로컬 데이터 수집은 AI 호출 없이, 종합 분석만 AI 사용. .journalist/quality/에 리포트 저장.
Issue: none

### Task 3: /template — 기사 양식 관리
Files: `src/commands_writing.rs`, `src/commands.rs`, `src/repl.rs`
Description: 기자별 커스텀 기사 양식 관리. `save <이름> [--file <경로>]` — 현재 초안 또는 지정 파일을 양식으로 저장. `list` — 저장된 양식 목록. `use <이름> <주제>` — 양식 기반 기사 작성 (양식 구조 + 주제를 AI에 전달). `show <이름>` — 양식 내용 확인. `remove <이름>` — 양식 삭제. .journalist/templates/에 마크다운으로 저장. /article --type이 "장르"라면 /template은 "내 글쓰기 패턴"이다.
Issue: none

### Task 4: journal entry
Files: `JOURNAL.md`
Description: 이번 세션의 작업과 배움을 기록.
Issue: none

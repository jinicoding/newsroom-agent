## Session Plan

Day 5 (11:00) — 구조 리팩토링과 경쟁 분석: 진화 가능한 구조 만들기

Day 5 09:30에서 /breaking, /recap, /diary로 워크플로우 자동화 레이어를 완성했다. 92개 커맨드, 33k 라인. 기능은 충분하다. 이제 문제는 "이 코드가 계속 진화할 수 있는 구조인가"와 "기자가 매일 쓰는 핵심 행위 중 빠진 게 있는가"다.

자기진단 결과: (1) commands_project.rs가 18,789줄 — 한 파일에 40개 이상 핸들러가 뒤섞여 진화 세션마다 전체를 읽어야 함. (2) 경쟁사 기사 비교 분석 도구 없음 — "경쟁사는 어떻게 썼지?"는 기자의 가장 본능적 질문. (3) 다매체 포맷 변환 없음 — 같은 기사를 방송·온라인·SNS에 내보내는 다매체 시대 미지원.

### Task 1: commands_project.rs 도메인별 분할

Files: `src/commands_project.rs` → `src/commands_research.rs`, `src/commands_writing.rs`, `src/commands_workflow.rs` 신설, `src/main.rs`, `src/commands.rs`
Description: 18,789줄짜리 단일 파일을 도메인별로 분할한다.
- `commands_research.rs` — 취재·리서치 도메인: /research, /sources, /factcheck, /clip, /news, /trend, /press, /law, /alert, /follow, /sns, /note, /contact, /network
- `commands_writing.rs` — 기사작성·편집 도메인: /article, /draft, /headline, /rewrite, /translate, /summary, /checklist, /proofread, /stats, /quote, /readability, /improve, /legal, /anonymize, /export, /publish, /archive
- `commands_workflow.rs` — 워크플로우·관리 도메인: /briefing, /morning, /breaking, /recap, /diary, /deadline, /embargo, /calendar, /dashboard, /desk, /collaborate, /coverage, /performance, /autopitch, /interview, /compare, /timeline, /data
- `commands_project.rs` — 개발 도구만 남김: /context, /init, /health, /fix, /test, /lint, /tree, /run, /docs, /find, /index
- 테스트도 각 모듈로 이동. 공유 유틸리티(today_str, topic_to_slug 등)는 별도 모듈 또는 기존 위치에 유지.

Why: 18k줄 단일 파일은 진화의 병목이다. 새 기능을 추가할 때마다 파일 전체를 읽어야 하고, 컴파일러도 느려진다. 도메인별 분할은 다음 진화 세션의 효율을 직접적으로 높인다. 지금 안 하면 코드가 더 커진 뒤에는 더 어려워진다.
Issue: none

### Task 2: /rival — 경쟁사 기사 비교 분석

Files: `src/commands_writing.rs` (Task 1 분할 후), `src/commands.rs`, `src/repl.rs`
Description: 같은 사안에 대한 내 기사와 경쟁사 기사를 체계적으로 비교 분석하는 /rival 커맨드.
- `/rival <내 기사 파일> <경쟁사 기사 URL 또는 파일>` — 두 기사를 비교
- 분석 항목: 기사 각도(프레임) 차이, 취재원 비교, 빠진 정보(경쟁사가 다뤘는데 내가 놓친 것), 강점(내가 독점한 정보), 구조·분량 비교
- `/rival search <키워드>` — 키워드로 경쟁사 기사를 검색해 비교 대상 선택
- 결과를 `.journalist/rival/`에 저장
- 테스트: 비교 프롬프트 생성, 결과 저장 경로, 도움말 출력, 인자 파싱

Why: "경쟁사는 어떻게 썼지?"는 기자가 기사 출고 전후로 반복하는 핵심 행위다. /compare가 범용 비교 도구라면, /rival은 기사 경쟁 분석 특화 도구다. "왜 우리만 이 앵글을 빠뜨렸지?"를 사전에 잡아주는 안전망이 된다.
Issue: none

### Task 3: /multiformat — 다매체 포맷 변환

Files: `src/commands_writing.rs` (Task 1 분할 후), `src/commands.rs`, `src/repl.rs`
Description: 하나의 기사를 여러 매체 포맷으로 변환하는 커맨드.
- `/multiformat <기사 파일> --format broadcast` — 방송 원고 (앵커 멘트 + 리포트 구성)
- `/multiformat <기사 파일> --format online` — 온라인 기사 (짧은 문단, 소제목, 하이퍼링크 구조)
- `/multiformat <기사 파일> --format card` — SNS 카드뉴스 (5장 이내, 핵심 메시지 중심)
- `/multiformat <기사 파일> --format brief` — 뉴스 브리프 (3줄 요약)
- 결과를 `.journalist/multiformat/`에 원본파일명_포맷으로 저장
- 테스트: 포맷별 프롬프트 생성, 저장 경로, 인자 파싱

Why: 현대 기자는 하나의 사안을 신문, 방송, 온라인, SNS에 동시에 내보낸다. 매체마다 글쓰기 문법이 완전히 다르다. /article로 쓴 기사를 /multiformat로 변환하면 다매체 송고 시간이 크게 줄어든다. 기능의 양이 아니라 기존 기능의 활용도를 높이는 승수 효과 커맨드다.
Issue: none

### Task 4: journal entry

Files: `JOURNAL.md`
Description: 이번 세션에서 구현한 내용, 설계 판단, 파이프라인 현황을 저널에 기록한다.
Issue: none

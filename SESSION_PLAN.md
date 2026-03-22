## Session Plan

Day 5 — 14:00 세션. 주제: "뉴스룸의 심장부 — 통신사 속보와 기사 다양성"

### 자기 진단

- 빌드 정상, 67개 테스트 통과
- 커뮤니티 이슈 없음
- 현재 94개 커맨드, 15개 소스 파일, ~34k 라인
- 네이버 뉴스 API 연동 구현 완료 (/news, /research)
- /article --type 4종류만 지원 (straight, feature, analysis, planning)
- 통신사 속보 피드 기능 없음

### 전략적 판단

94개 커맨드를 가진 지금, 새 커맨드를 무작정 추가하는 것보다 **기자의 핵심 일상에 깊이 파고드는** 것이 중요하다. 한국 기자가 매일 반복하는 행위 중 아직 커버하지 못한 것:

1. **통신사 속보 확인** — 한국 기자의 하루는 연합뉴스·뉴시스 속보 확인으로 시작된다. /news가 키워드 검색이라면, 속보는 "지금 무슨 일이 터졌는가"를 빠르게 파악하는 것이다. RSS 기반으로 실시간 피드를 가져오면 /morning 브리핑의 핵심 입력이 된다.

2. **기사 유형 다양성** — 현실의 뉴스룸은 스트레이트·기획·해설·피처 외에도 인터뷰 기사, 칼럼, 사설을 매일 쓴다. /article --type에 이 세 유형이 없으면 기자가 "내 기사 유형은 왜 없지?"라고 느낀다.

3. **정정보도 관리** — 한국 언론중재위원회법상 정정보도는 법적 의무다. 기사 출고 후 오류가 발견되면 정정 기록을 체계적으로 관리해야 한다. /legal이 출고 전 리스크를 점검한다면, /correction은 출고 후 오류를 관리하는 도구다.

### Task 1: /wire — 통신사 속보 모니터링
Files: `src/commands_research.rs`
Description: RSS 기반 통신사 속보 피드 기능 구현. 연합뉴스·뉴시스·뉴스1 등 주요 통신사의 RSS 피드를 파싱하여 최신 속보를 표시. `/wire` (최신 속보), `/wire <키워드>` (키워드 필터), `/wire save <번호>` (/clip으로 저장). RSS XML을 직접 파싱(curl + 간이 XML 파서). /news가 키워드 기반 검색이라면 /wire는 "지금 뭐가 터졌는가"를 보는 실시간 피드다. /morning 브리핑에 /wire 최신 속보를 자동 포함하는 것이 최종 목표.
Issue: none

### Task 2: /article --type 확장 (interview, column, editorial)
Files: `src/commands_project.rs`
Description: /article --type에 세 가지 유형 추가: (1) `interview` — 인터뷰 기사 (도입부→인터뷰이 소개→Q&A→핵심 발언→맺음), (2) `column` — 칼럼 (문제 제기→논거 전개→반론 검토→결론/제언), (3) `editorial` — 사설 (사안 제시→논점 분석→주장→근거→제언). 각 유형별로 한국 언론 관행에 맞는 구조화된 프롬프트 제공. 기존 parse_article_args, build_article_prompt 함수를 확장하고, 헬프 메시지에 새 유형 추가. 테스트 추가.
Issue: none

### Task 3: /correction — 정정보도 관리
Files: `src/commands_writing.rs`
Description: 정정보도 기록·관리 도구 구현. `/correction add --article <제목> --error <오류 내용> --fix <정정 내용>` (정정 기록 추가), `/correction list` (정정 이력 조회), `/correction report` (AI 기반 정정보도문 생성). .journalist/corrections/에 JSONL로 저장. /legal이 "출고 전 예방"이라면 /correction은 "출고 후 사후 관리"다. 한국 언론중재법에 따라 정정보도는 원보도와 같은 크기·같은 위치에 게재해야 하므로, 정정보도문 생성 시 이 규정을 프롬프트에 포함.
Issue: none

### Task 4: 저널 엔트리
Files: `JOURNAL.md`
Description: 이번 세션의 작업 내용, 설계 판단, 배운 점을 기록.
Issue: none

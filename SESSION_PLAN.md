## Session Plan — Day 1 (2026-03-18)

### 자기 평가 요약

- **빌드/테스트:** 성공 (67 tests, 0 failed)
- **커뮤니티 이슈:** 없음
- **현재 상태:** Day 0에서 /article 저장, /research 캐싱, /sources CRUD, 테스트 보강 완료

**발견한 문제점:**
1. `/factcheck` 결과가 파일로 저장되지 않음 — /article, /research와의 일관성 부재
2. `/research` curl+sed HTML 파싱이 네이버 검색 결과를 제대로 가져오지 못함
3. 보도자료 → 기사 변환 워크플로우 없음 (한국 기자 가장 빈번한 작업)
4. /article에서 기존 /research 결과를 참조하지 못함 (리서치→기사 흐름 단절)

**리서치 (한국 뉴스룸 자동화):**
- 네이버 뉴스 검색 API 무료 사용 가능 (developer.naver.com 등록)
- BIGKinds(빅카인즈) API — 104개 매체 아카이브, NLP 기능 내장
- 보도자료→기사 변환이 조선일보 등 핵심 AI 도입 사례
- 팩트체크 도구는 "근거 보이기"가 필수 — 기자가 불투명한 결과는 안 씀

---

### Task 1: /factcheck 결과 저장 + list 하위 명령
Files: `src/commands_project.rs`
Description: /factcheck 결과를 `.journalist/factcheck/YYYY-MM-DD_<slug>.md`에 저장. `/factcheck list`로 저장된 팩트체크 목록 조회. 기존 research 저장 패턴(save 함수 + 디렉토리 생성 + 성공 메시지)을 그대로 따름.
테스트: factcheck 파일 경로 생성, 저장 함수 단위 테스트
Issue: none

### Task 2: /briefing 커맨드 — 보도자료 요약 변환
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 보도자료(텍스트 또는 파일)를 기사 초안으로 변환하는 `/briefing` 커맨드. 한국 기자의 가장 빈번한 일상 업무. `--file <경로>`로 파일에서 읽거나 인라인 입력. 프롬프트: 보도자료 핵심 사실 추출 → 역피라미드 구조 기사 초안 → [확인 필요] 마킹. 결과를 `.journalist/drafts/`에 저장. KNOWN_COMMANDS와 REPL에 등록.
테스트: briefing 프롬프트 생성, 파일 읽기 로직, KNOWN_COMMANDS 등록 확인
Issue: none

### Task 3: /article에서 기존 리서치 자동 참조
Files: `src/commands_project.rs`
Description: /article 실행 시 `.journalist/research/`에서 주제와 관련된 리서치 파일을 검색하여 프롬프트 맥락에 포함. 리서치→기사 워크플로우의 자연스러운 연결. 파일명의 slug와 주제 키워드를 매칭하여 관련 파일 탐색.
테스트: 리서치 파일 검색 로직, 프롬프트에 맥락 포함 여부 테스트
Issue: none

### Task 4: /factcheck 교차검증 프롬프트 강화
Files: `src/commands_project.rs`
Description: 현재 프롬프트에 구체적 교차검증 전략 추가: (1) 공공데이터포털(data.go.kr) 통계 확인, (2) 공식 보도자료 대조, (3) 시계열 데이터 비교, (4) 검증 과정을 단계별로 보여주기(기자가 근거 없는 판정은 안 씀). 리서치에서 발견한 "Show Me the Work" 원칙 반영.
테스트: 강화된 프롬프트에 새 키워드(data.go.kr, 보도자료 대조 등) 포함 확인
Issue: none

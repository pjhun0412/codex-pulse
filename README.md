# Codex Pulse

Windows 작업 표시줄 왼쪽에 고정하는 Codex 사용량 및 시스템 상태 위젯입니다.

## 다운로드

[GitHub Releases에서 최신 Windows 설치 프로그램 다운로드](https://github.com/pjhun0412/codex-pulse/releases/latest)

현재 설치 파일은 디지털 서명되지 않았습니다. 실행할 때 Windows SmartScreen 경고가
표시될 수 있습니다.

## 현재 구현

- Tauri 2 + React 19 + TypeScript + Rust
- 작업 표시줄 좌표 자동 감지 및 고정
- 트레이에서 네이티브 작업표시줄 모드와 자유 오버레이 모드 전환
- 우클릭 메뉴에서 선택하는 Windows 시작 시 자동 실행
- 접힌 2×2 매트릭스: Codex 한도, 초기화 시간, CPU, 메모리
- 클릭 시 상세 패널 확장
- Codex app-server의 계정, 한도, 토큰 활동 조회
- 로그인 또는 app-server 연결 실패 시 수치를 `-`로 표시
- CPU·메모리 1초 갱신, Codex 데이터 30초 갱신
- 시스템 트레이 표시·숨김·새로고침·종료
- 창 닫기 시 종료 대신 트레이로 숨김

구독 결제일이나 만료일은 Codex app-server가 제공하지 않으므로 표시하지 않습니다.

## 개발 실행

```powershell
npm install
npm run tauri:dev
```

브라우저에서 UI만 확인:

```powershell
npm run dev
```

## 빌드

```powershell
npm run build:frontend
npm run tauri:build
```

Windows 설치 파일은 `src-tauri/target/release/bundle/` 아래에 생성됩니다.

## 실제 데이터 연결

Rust의 `CodexProvider`가 `codex app-server`를 표준 입출력으로 실행하고 다음 요청을 사용합니다.

- `account/read`
- `account/rateLimits/read`
- `account/usage/read`

위젯은 Codex Desktop의 실행 가능한 로컬 사본을 자동 탐색합니다. ChatGPT 구독
계정으로 로그인되어 있으면 실제 데이터를 표시하며, 조회할 수 없는 값은 추정하지
않고 `-`로 표시합니다.

## 배치 모드

트레이 아이콘을 마우스 오른쪽 버튼으로 눌러 선택합니다.

- `작업표시줄에 붙이기`: Win32 `WS_CHILD` 창을 Explorer 작업표시줄 내부에 붙입니다. 위젯 자체의 최상위 강제를 사용하지 않고 작업표시줄의 표시 상태와 창 순서를 따릅니다. Explorer가 작업표시줄을 다시 만들면 자동으로 재연결합니다.
- `자유 오버레이`: 작업 영역 왼쪽 아래에 띄우며 상세 패널의 상단을 드래그해 이동할 수 있습니다.

미니 위젯과 상세 패널은 별도 네이티브 창입니다. 따라서 팝업을 열고 닫을 때
미니 창을 리사이즈하지 않으며, 바깥을 클릭하면 상세 패널이 자동으로 닫힙니다.

작업표시줄 삽입 방식과 검증 범위는 [작업표시줄 위젯 문제 해결 기록](docs/incidents/2026-09-08-taskbar-child-window.md)을 참고하세요.

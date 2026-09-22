pub const APP_TITLE: &str = "GitHub Action Console";
pub const LABEL_VERSION: &str = "版本";
pub const LABEL_BUILD_TARGET: &str = "构建目标";
pub const LABEL_PACKAGING_CONFIG: &str = "打包配置";
pub const UNSPECIFIED: &str = "未指定";

pub const LOGIN_TITLE: &str = "使用 OAuth 登录";
pub const LOGIN_START: &str = "使用 OAuth 登录";
pub const LOGIN_STARTING: &str = "正在请求授权…";
pub const LOGIN_WAITING: &str = "等待授权中…";
pub const LOGIN_INSTRUCTION: &str = "请在浏览器中打开下面的地址，并输入一次性代码：";
pub const LOGIN_USER_CODE: &str = "一次性代码";
pub const LOGIN_COPY: &str = "复制代码";
pub const LOGIN_COPIED: &str = "已复制";
pub const LOGIN_OPEN_BROWSER: &str = "打开验证地址";
pub const LOGIN_EXPIRES_IN: &str = "有效期（秒）";
pub const LOGIN_LOGGED_IN_AS: &str = "当前登录用户";
pub const LOGIN_SIGN_OUT: &str = "登出";

pub const NOTICE_DEVICE_FLOW_UNAVAILABLE: &str =
    "未配置 OAuth App 的 client_id，无法使用 Device Flow 登录。";
pub const NOTICE_MISSING_CLIENT_ID: &str = "请输入 Client ID";
pub const NOTICE_EXPIRED: &str = "授权码已过期，请重新登录。";
pub const NOTICE_DENIED: &str = "授权已被拒绝。";
pub const NOTICE_INVALID_CREDENTIALS: &str = "登录凭据已失效，请重新登录。";
pub const NOTICE_NETWORK: &str = "网络请求失败，请检查网络后重试。";
pub const NOTICE_UNEXPECTED: &str = "出现未预期的错误，请重试。";
pub const NOTICE_MISSING_SCOPES: &str = "凭据权限不足。需要 repo + workflow（粗粒度 PAT），或 Contents: RW、Actions: RW、Workflows: RW（细粒度 PAT）。";

pub const LOGIN_CLIENT_ID_PLACEHOLDER: &str = "请输入 OAuth Client ID";
pub const LOGIN_PAT_BUTTON: &str = "使用 Personal Access Token 登录";
pub const LOGIN_PAT_SUBMIT_ARROW: &str = "→";
pub const LOGIN_PAT_BACK: &str = "←";
pub const LOGIN_PAT_PLACEHOLDER: &str = "请输入 Personal Access Token";
pub const LOGIN_VALIDATING: &str = "正在验证凭据…";

pub const REPOSITORIES_TITLE: &str = "仓库";
pub const REPOSITORIES_SEARCH_PLACEHOLDER: &str = "按名称搜索仓库";
pub const REPOSITORIES_SORT_UPDATED: &str = "按最近更新";
pub const REPOSITORIES_SORT_PUSHED: &str = "按最近推送";
pub const REPOSITORIES_LOAD_MORE: &str = "加载更多";
pub const REPOSITORIES_LOADING: &str = "正在加载仓库…";
pub const REPOSITORIES_EMPTY: &str = "没有匹配的仓库。";
pub const REPOSITORIES_PRIVATE: &str = "私有";
pub const REPOSITORIES_PUBLIC: &str = "公开";
pub const REPOSITORIES_COLUMN_NAME: &str = "仓库";
pub const REPOSITORIES_COLUMN_VISIBILITY: &str = "可见性";
pub const REPOSITORIES_COLUMN_BRANCH: &str = "默认分支";
pub const REPOSITORIES_COLUMN_COMMIT: &str = "最近提交";
pub const REPOSITORIES_COLUMN_COMMIT_DATE: &str = "提交日期";

pub const WORKSPACE_BACK: &str = "返回仓库列表";
pub const WORKSPACE_BACK_ARROW: &str = "←";
pub const WORKSPACE_WORKFLOWS: &str = "工作流";
pub const WORKSPACE_RUNS: &str = "运行";
pub const WORKSPACE_COMING_SOON: &str = "即将推出。";

pub const PROBLEM_RATE_LIMITED: &str = "已触发 GitHub 限流，请稍后重试。";
pub const PROBLEM_NOT_FOUND: &str = "该仓库不存在或无权访问。";

pub const WORKFLOWS_LOADING: &str = "正在加载工作流…";
pub const WORKFLOWS_EMPTY: &str = "该仓库没有工作流。";
pub const RUNS_LOADING: &str = "正在加载运行…";
pub const RUNS_EMPTY: &str = "没有匹配的运行。";
pub const RUNS_LOAD_MORE: &str = "加载更多运行";
pub const RUNS_COLUMN_STATUS: &str = "状态";
pub const RUNS_COLUMN_NAME: &str = "名称";
pub const RUNS_COLUMN_BRANCH: &str = "分支";
pub const RUNS_COLUMN_EVENT: &str = "事件";
pub const RUNS_COLUMN_ACTOR: &str = "触发者";
pub const RUNS_COLUMN_CREATED: &str = "时间";
pub const RUNS_FILTER_ALL: &str = "全部";
pub const RUNS_FILTER_RUNNING: &str = "进行中";
pub const RUNS_FILTER_COMPLETED: &str = "已完成";
pub const RUNS_FILTER_WORKFLOW_ALL: &str = "全部工作流";
pub const RUNS_BRANCH_PLACEHOLDER: &str = "按分支过滤";
pub const VALUE_MISSING: &str = "—";

pub const RUN_DETAIL_BACK: &str = "返回运行列表";
pub const RUN_DETAIL_OPEN_BROWSER: &str = "在浏览器打开";
pub const JOBS_TITLE: &str = "Jobs";
pub const JOBS_LOADING: &str = "正在加载 job…";
pub const JOBS_EMPTY: &str = "该运行没有 job。";
pub const JOB_VIEW_LOGS: &str = "查看日志";
pub const JOB_STEPS: &str = "步骤";
pub const LOGS_TITLE: &str = "日志";
pub const LOGS_LOADING: &str = "正在加载日志…";
pub const LOGS_EMPTY: &str = "该 job 没有日志。";
pub const LOGS_COPY: &str = "复制日志";
pub const LOGS_COPIED: &str = "已复制";
pub const LOGS_SEARCH_PLACEHOLDER: &str = "在日志中搜索";

pub const ARTIFACTS_TITLE: &str = "构建产物";
pub const ARTIFACTS_LOADING: &str = "正在加载构建产物…";
pub const ARTIFACTS_EMPTY: &str = "该运行没有构建产物。";
pub const ARTIFACTS_EXPIRED: &str = "已过期";
pub const ARTIFACT_DOWNLOAD: &str = "下载";
pub const RUN_LOGS_DOWNLOAD: &str = "下载运行日志包";
pub const DOWNLOAD_CONFIRM_TITLE: &str = "该文件较大，确认下载？";
pub const DOWNLOAD_CONFIRM: &str = "确认下载";
pub const DOWNLOAD_CANCEL: &str = "取消";
pub const DOWNLOADING: &str = "正在下载…";
pub const DOWNLOAD_SAVED: &str = "已保存到";

pub const STATUS_BAR_ACCOUNT: &str = "登录用户";
pub const STATUS_BAR_RATE_LIMIT: &str = "限流余量";
pub const STATUS_BAR_SIGNED_OUT: &str = "未登录";
pub const NOTICES_DISMISS: &str = "清除通知";

pub const SETTINGS_TITLE: &str = "设置";
pub const SETTINGS_GROUP_TITLE: &str = "网络";
pub const SETTINGS_PROXY_LABEL: &str = "代理地址";
pub const SETTINGS_PROXY_PLACEHOLDER: &str = "例如 http://127.0.0.1:7890";
pub const SETTINGS_SAVE: &str = "保存";
pub const SETTINGS_CLOSE: &str = "关闭";

pub const RESUME_TITLE: &str = "登录仍然有效";
pub const RESUME_BODY: &str = "上次登录仍然有效，是否直接进入应用？";
pub const RESUME_YES: &str = "是";
pub const RESUME_NO: &str = "否";

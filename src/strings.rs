pub const APP_TITLE: &str = "GitHub Action Console";
pub const LABEL_VERSION: &str = "版本";
pub const LABEL_BUILD_TARGET: &str = "构建目标";
pub const LABEL_PACKAGING_CONFIG: &str = "打包配置";
pub const UNSPECIFIED: &str = "未指定";

pub const LOGIN_TITLE: &str = "登录 GitHub";
pub const LOGIN_START: &str = "使用 GitHub 登录";
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
pub const NOTICE_EXPIRED: &str = "授权码已过期，请重新登录。";
pub const NOTICE_DENIED: &str = "授权已被拒绝。";
pub const NOTICE_INVALID_CREDENTIALS: &str = "登录凭据已失效，请重新登录。";
pub const NOTICE_NETWORK: &str = "网络请求失败，请检查网络后重试。";
pub const NOTICE_UNEXPECTED: &str = "出现未预期的错误，请重试。";
pub const NOTICE_MISSING_SCOPES: &str = "凭据权限不足。需要 repo + workflow（粗粒度 PAT），或 Contents: RW、Actions: RW、Workflows: RW（细粒度 PAT）。";

pub const LOGIN_PAT_TITLE: &str = "或使用 Personal Access Token 登录";
pub const LOGIN_PAT_PLACEHOLDER: &str = "粘贴 token";
pub const LOGIN_PAT_SUBMIT: &str = "使用 token 登录";
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

pub const WORKSPACE_BACK: &str = "返回仓库列表";
pub const WORKSPACE_WORKFLOWS: &str = "工作流";
pub const WORKSPACE_RUNS: &str = "运行";
pub const WORKSPACE_COMING_SOON: &str = "即将推出。";

pub const PROBLEM_RATE_LIMITED: &str = "已触发 GitHub 限流，请稍后重试。";
pub const PROBLEM_NOT_FOUND: &str = "该仓库不存在或无权访问。";

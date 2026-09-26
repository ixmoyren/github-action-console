//! 仓库里那份发布脚本，控制台可以直接采用。
//!
//! 模板只有一份：仓库根的 `.github/workflows/release-target.yml`。这里用
//! `include_str!` 把它编译进来，控制台写进目标仓库的就是同一段文本——不复制第二份，
//! 免得模板、文档和实际跑的脚本各说各话。

/// 模板在目标仓库里的位置。
pub const TEMPLATE_PATH: &str = ".github/workflows/release-target.yml";

/// 模板原文，编译期从仓库里读进来。
pub const TEMPLATE: &str = include_str!("../.github/workflows/release-target.yml");

/// 给某个仓库的那一份：开头写上这是给谁生成的，正文一字不改。
///
/// 模板本身不知道自己在哪个仓库里跑，采用它的仓库才是模板的上下文；把仓库名写进
/// 生成物，人打开文件时一眼知道这份是从哪来的。
pub fn for_repository(repository: &str) -> String {
    format!("# 由 GitHub Action Console 为 {repository} 生成。\n{TEMPLATE}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_covers_the_four_platforms() {
        // 四个平台各有一个 runner 与目标名；少了哪一个，模板就名不副实。
        for needle in ["macos-14", "windows-latest", "ubuntu-latest"] {
            assert!(TEMPLATE.contains(needle), "模板里没有 {needle}");
        }
        for needle in ["macos-arm", "macos-intel", "windows", "linux"] {
            assert!(TEMPLATE.contains(needle), "模板里没有内建目标 {needle}");
        }
    }

    #[test]
    fn the_template_uploads_its_builds_to_a_release() {
        assert!(TEMPLATE.contains("gh release create"), "模板不会建 Release");
        assert!(TEMPLATE.contains("gh release upload"), "模板不会上传产物");
        assert!(
            TEMPLATE.contains("upload-artifact"),
            "模板没有构建产物这一步"
        );
    }

    #[test]
    fn adopting_the_template_names_the_repository() {
        let rendered = for_repository("octo/alpha");
        assert!(rendered.starts_with("# 由 GitHub Action Console 为 octo/alpha 生成。"));
        // 正文就是模板本身，一字不改。
        assert!(rendered.ends_with(TEMPLATE));
    }

    #[test]
    fn the_template_lands_where_the_console_reads_workflows() {
        assert!(TEMPLATE_PATH.starts_with(".github/workflows/"));
        assert!(TEMPLATE_PATH.ends_with(".yml"));
    }
}

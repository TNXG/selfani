# Claude Code 规范

以下内容为项目中使用 Claude Code 进行代码生成和修改的规范和最佳实践，涵盖了提交信息规范、代码风格、项目结构等方面，旨在确保代码质量和团队协作效率。

如果与全局规范冲突，请以本文件为准。

## 规范

### Commit 规范

请严格遵循 **Conventional Commits** 规范：

#### 格式

- `<type>(<scope>): <description>`

- **type**: `feat` | `fix` | `docs` | `style` | `refactor` | `perf` | `test` | `build` | `ci` | `chore` | `revert`
- **scope**: 可选，简短描述受影响模块（如 `auth`、`api`、`ui`）。用 camelCase 或 kebab-case。
- **description**: 简洁、祈使句、现在时，不超 50 字符，首字母小写，末尾无句号，用无序列表分列。
- **可选正文/脚注**: 额外上下文或 `BREAKING CHANGE: ...`。
- **中文描述**，使用简体中文生成commit message信息。

#### 原子提交

- 每个 commit **只包含一个功能/修复**。
- 如果涉及多个功能/模块，**必须拆分成多个 commit**，每个都符合上述规范。

#### 示例

```
feat(docker): 配置私有仓库和私有镜像支持
- 修改GitHub工作流文件，使用固定的私有仓库 ave-mygo/ink-battles
- 更新docker-compose.yml使用GitHub Container Registry私有镜像
- 注释掉原有的CNB镜像配置，保留备份
- 确保Docker镜像推送到私有仓库并正确引用
```
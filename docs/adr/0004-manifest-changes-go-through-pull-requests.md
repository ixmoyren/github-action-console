# Manifest changes go through a branch and pull request, never to the default branch

The console itself writes each repo's release manifest, and a direct `update_file` against the default branch would mutate a user's branch (often protected) from a desktop GUI. Every manifest write therefore lands on a new branch and is opened as a pull request, so the change is reviewable and revertible under the repo's normal protections; when a repo has no manifest yet, the app generates an initial one through the same path. Considered committing straight to the default branch (rejected: bypasses
protection and review) and exporting locally for the user to commit (rejected: hands mechanical work back to the user and breaks the round trip).

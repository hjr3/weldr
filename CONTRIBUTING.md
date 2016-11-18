# Contributing

If you are looking to help to with a code contribution our project uses the Rust programming language with the tokio asynchronous IO framework. If you don't feel ready to make a code contribution yet, no problem! You can also check out the [documentation issues](https://github.com/hjr3/alacrity/labels/documentation).

If you need help, you can ask questions IRC chat. I am _hjr3_ on irc.mozilla.org and freenode.

## Feature Requests

To request a change to the way that the alacrity works, please open an issue before starting work. New features and other significant changes must be discussed first.

## Pull Requests

Pull requests are the primary mechanism we use to change alacrity. GitHub itself has some [great documentation](https://help.github.com/articles/about-pull-requests/) on using the Pull Request feature. We use the _fork and pull_ model described there.

A few additional guidelines:

   * Please make pull requests against the `master` branch. Multiple commits are fine as long as each commit has a good commit message and passes the tests.
   * Please rebase instead of merging changes from the `master` branch.
   * Please make sure the first line of the Git commit messages are no longer then 72 characters. The [Git documentation](https://www.kernel.org/pub/software/scm/git/docs/git-commit.html#_discussion) suggests 50 characters.

For more information about good commit messages see [Tbaggery](http://tbaggery.com/2008/04/19/a-note-about-git-commit-messages.html) or [MediaWiki](https://www.mediawiki.org/wiki/Gerrit/Commit_message_guidelines).

## Code of Conduct

All contributors are expected to follow the [Rust Code of Conduct](https://www.rust-lang.org/en-US/conduct.html).

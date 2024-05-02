# nvim-matrix-bot

Currently just supports replying to messages with `:h <some_doc>` or similar in them with a link
to the docs on Neovim's website. Plan is probably to make it more complex than that though, with support
for commands and such.

# Usage

This bot works by syncing up with whatever rooms it's in and then running its commands for each applicable message from each room it's in.
In other words, if you pass *your* username and password to the environmental variables `MATRIX_USERNAME` and `MATRIX_PASSWORD`
respectively when running the program, your account becomes the bot.

So if you want to use this in your own room, just create an account for it (or sign into your own) and then pass that in when running the bot.
`MATRIX_USERNAME=my-bot-username MATRIX_PASSWORD=my-bot-password ./nvim-matrix-bot` or whatever.

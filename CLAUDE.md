Important:
* double check which dependency versions to use
* maintain up to date docs
* keep the codebase clean (e.g. clean up duplicated code)


Follow Roblox's approach:
- Engine is completely generic
- All game-specific logic lives in Lua scripts

Make sure the postgress database is up to date.

Use shadcn for the frontend.
Consider using icons instead of text for professional-looking UI.

Don't silently swallow errors. Raise warnings.
Don't duplicated hard coded values. That's very bug prone.


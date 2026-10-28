module example.com/polyglot/service

go 1.22

require example.com/polyglot/schema v0.0.0

// The generated package, where buildl places the Go output of //schema:gen.
replace example.com/polyglot/schema => ../out/schema/gogen

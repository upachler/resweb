 # resweb ![build:status](https://travis-ci.com/upachler/resweb.svg?branch=master&amp;status=passed) 
 
 A central dashboard designed to dynamically link to other web apps in an intraweb.
 
 * Only lists sites that a user has access to
 * Connects to an OpenID Connect authorization server (IDP) issuing JWT access tokens (tested with [Keycloak](https://www.keycloak.org/))
 * Analyses the JWT access token of a logged in user to determine which links to display
 * Dashboard page is fully customizable via  [handlebars](http://handlebarsjs.com/) templates
 * Written in [Rust](https://rust-lang.org)
  


## Installation ##

Resweb is currently available in source only. To build, you'll need a recent version of Rust which you can get from [rustup.rs](https://rustup.rs/) (don't be shy, installation is effortless), as well as a [git](https://git-scm.com/) client.

Don't forget that you'll also need an authorization server (IDP, we use [Keycloak](https://www.keycloak.org/), but any other that issues JWT access tokens with configurable claims should work as well).

Clone this repository with your git client...

```sh
git clone <resweb-url> resweb
```

and build resweb with cargo:

```sh
cargo build --release
```

I everthing went well, you should be able to find the resweb executable in `./target/release/resweb`. Copy it whereever you need, there are no other files that it requires for an initial run. Smoke test it by running it with the `help` command:

```sh
resweb help
```

 ## Getting started ##
 
 The following assumes you have a [Keycloak](https://www.keycloak.org/) authorization server running on `http://localhost:8080`, with a realm called `test` configured. You can download it from the website, unzip the package and run it locally by starting with with `./bin/standalone.sh` (more details on the website).

 Resweb has several subcommands (run `resweb help` and `resweb help <command>` for details). The most important is `serve`, which runs resweb in server mode. The `serve` subcommand requires a configuration file, which at least must contain
 * URL and clientId for the authorization server
 * port on which resweb should listen for incoming connections
 * an empty site list

A minimum configuration looks like this:
```yaml
#
# resweb.yaml
#
# port on which resweb should listen for incoming connections
port: 8081
# base URI of the authorization server (IPD). resweb  
# will append '/.well-known/openid-configuration' to 
# this URI and load more configuration from there. 
# The IDP must support support OpenID Connect Discovery
authorization_server_url: http://localhost:8080/auth/realms/test
client_id: resweb
# OpenID scopes to use, may be needed to make role 
# claims visible in access token
scope: 'openid roles'
# a list of site links to display on the dashboard page
site_list:
  sites: []
```

To run resweb with this config file named `resweb.yaml`, you start resweb with:
```
resweb serve resweb.yaml
```

Resweb now serves web request on the configured port, so we should simply be able to open http://localhost:8081 in the browser.
When doing so, we're immediately redirected to the IDP's login screen. If you log in with proper user credentials, you'll be redirected back to resweb, which displays its dashboard screen. 

However, because we didn't configure any sites yet, none are displayed, because the `sites` array in our config file is empty:

```yaml
...
site_list:
  sites: []
...
```
In your intraweb, you'll have web applications like wiki, billing/time booking, project management, web mail, bug tracking, etc. To have them listed in resweb, we need to add them to the configuration file. Before displaying the site list on the dashboard, resweb filters it by matching site specific filtering rules that take the user's access token as input. Only sites for which at least one of their rules match are shown on the dashboard.

When these sites use the IDP for the login, it will have roles configured for them. For instance, to access the wiki, roles like `wiki_user` or `wiki_admin` will exist. Site rules can then check if one of these role names is included in the access token. When requesting the `roles` scope, Keycloak will put the role names in the `realm_access.roles` and `client_access.roles` claims (depending on whether roles are defined globally or specifically for a client).

Assuming we have a the wiki accessible via `https://wiki.inraweb.local/`, and that it uses Keycloak's realm roles, we have a site list like this:

```yaml
...
site_list:
  sites:
  - name: Wiki
    url: https://wiki.inraweb.local/
    claim_rules:
    - path: 'realm_access.roles'
      operator: ContainsMatch
      operand:
        value: wiki_user
    - path: 'realm_access.roles'
      operator: ContainsMatch
      operand:
        value: wiki_admin
...
```

From the example you see that each site consists of a `name`, a `url` and a list of `claim_rules`. The `name` is displayed in the dashboard if any of the `claim_rules` match. 
Each rule itself consists of a `path` (JSON path, also called claim name), the comparson `operator` and the `value` to compare the contents of the path with. `operator` can currently be `Matches` (for paths that hold a single value) or `ContainsMatch` (for paths holding an array of values, like the `realm_access.roles` path in our example). As you can see, we put the role names in to the `value` of each of the rules, so if a use has any of these roles assigned, the site will be displayed.

As you see, enumerating all possible roles as separate rules is quite cumbersome. For cases like the one above, where all roles start with the same substring `wiki_`, we can also use regular expressions (regex) to make things shorter - however, the resulting rule will then match all role names starting with `wiki_`:
```yaml
...
site_list:
  sites:
  - name: Wiki
    url: https://wiki.inraweb.local/
    claim_rules:
    - path: 'realm_access.roles'
      operator: ContainsMatch
      operand:
        # matches all roles that start with 'wiki_'
        regex: wiki_.*
...
```

The `regex` operand uses perl-like regular expressions. As resweb uses the `regex` crate, it adheres to it's [syntax described here](https://docs.rs/regex/1.5/regex/#syntax).

The example above assumes that you have a site like this running, which you might not have locally. To provide an example running out-of the box, assume we make Google and Disney part of our intraweb. Users who can access Disney will need the `disney` role, while the ones allowed to do Google searches require the `google` role.

```yaml
port: 8081
authorization_server_url: http://localhost:8080/auth/realms/test
client_id: resweb
scope: 'openid roles'
site_list:
  sites:
  - name: Disney
    url: https://www.disney.com/
    claim_rules:
    - path: 'realm_access.roles'
      operator: ContainsMatch
      operand:
        value: disney
  - name: Google
    url: https://www.google.com/
    claim_rules:
    - path: 'realm_access.roles'
      operator: ContainsMatch
      operand:
        value: google
```

## Customization ##

Resweb comes with a set of built-in template that are ok for a first look, but you'll surely want to customize them to match your company's look and feel. Resweb allows you to do that by
* loading templates from an external template directory
* dynamic template reloading if started in dev mode

To start customization, we need a starting point. To provide that, resweb can export its internal templates into a local directory. 

If you run...
```sh
resweb init-templates
```
... resweb will export its templates into the default `templates/` directory. When started later with the `serve` command, it will discover that `templates/` is present in the current directory and load the templates from there. If a required file is not in the templates directory, it will be loaded from the internal file store.

When you now look into the `templates/` directory, you'll see the default templates. You can now edit them as you please. If you need static files like images, etc., simply add them here and reference them from your templates using relative paths.

If you need the template directory to be somewhere else, you can specify its location with the `-t` command line switch (run `resweb help` for details).

On a normal run of `resweb serve`, templates are only loaded on startup. To make life easier during development, the `serve` subcommand has a switch to enable dev mode, which will reload the templates from disk as soon as they are changed. All you need to do after changing your templates is to refresh your browser window. To develop using a `resweb.yaml` configuration file from an arbitrary template directory, use

```
resweb --template-dir path/to/templates serve --development resweb.yaml
```

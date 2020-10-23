.PHONY: pretty

pretty:
	docker run --interactive --rm --user $(shell id --user):$(shell id --group) pandoc/core:2.11.0.2 < out/security_monkey_case_files.html --toc --from html --to html --standalone > out/security_monkey_case_files_pretty.html

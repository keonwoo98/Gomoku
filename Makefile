NAME = Gomoku

PYTHON = python3
MAIN = main.py

all: $(NAME)

$(NAME): requirements
	@echo "#!/bin/bash" > $(NAME)
	@echo 'cd "$$(dirname "$$0")" && $(PYTHON) $(MAIN) "$$@"' >> $(NAME)
	@chmod +x $(NAME)
	@echo "Build complete: ./$(NAME)"

requirements:
	@$(PYTHON) -m pip install -q -r requirements.txt 2>/dev/null || \
		$(PYTHON) -m pip install -q pygame 2>/dev/null || \
		echo "Warning: Could not install pygame automatically"

run: all
	./$(NAME)

clean:
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true

fclean: clean
	rm -f $(NAME)

re: fclean all

test:
	$(PYTHON) -m pytest tests/ -v

.PHONY: all clean fclean re run test requirements

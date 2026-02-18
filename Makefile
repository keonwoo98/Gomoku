NAME = Gomoku

# Prefer rustup toolchain, fall back to PATH
CARGO ?= $(shell rustup which cargo 2>/dev/null || which cargo 2>/dev/null)
RUSTC ?= $(shell rustup which rustc 2>/dev/null || which rustc 2>/dev/null)

all: $(NAME)

$(NAME): FORCE
	@RUSTC=$(RUSTC) $(CARGO) build --release
	@cp target/release/gomoku $(NAME)

FORCE:

clean:
	@RUSTC=$(RUSTC) $(CARGO) clean

fclean: clean
	@rm -f $(NAME)

re: fclean all

test:
	@RUSTC=$(RUSTC) $(CARGO) test --lib

test-release:
	@RUSTC=$(RUSTC) $(CARGO) test --lib --release

.PHONY: all clean fclean re test test-release

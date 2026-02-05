NAME = Gomoku

all: $(NAME)

$(NAME): FORCE
	@cargo build --release
	@cp target/release/gomoku $(NAME)

FORCE:

clean:
	@cargo clean

fclean: clean
	@rm -f $(NAME)

re: fclean all

test:
	@cargo test --lib

test-release:
	@cargo test --lib --release

.PHONY: all clean fclean re test test-release

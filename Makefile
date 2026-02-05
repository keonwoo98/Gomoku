NAME = Gomoku

all: $(NAME)

$(NAME):
	@cd engine && cargo build --release
	@cp engine/target/release/gomoku $(NAME)

clean:
	@cd engine && cargo clean

fclean: clean
	@rm -f $(NAME)

re: fclean all

.PHONY: all clean fclean re

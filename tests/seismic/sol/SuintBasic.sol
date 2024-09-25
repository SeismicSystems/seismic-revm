contract SuintBasic {
    suint256 x;

    function test() public {
        // can set x using literal
        x = 3;
        assert(x == 3);

        // can set x using uint256
        uint256 y = 5;
        x = y;
        assert(x == 5);

        // can set x using suint256
        suint256 z = 6;
        x = z;
        assert(x == 6);

        // can operate on x with literal
        x = 0;
        x += 3;
        assert(x == 3);

        // can operate on x with uint256
        x = 0;
        x += y;
        assert(x == 5);

        // can operate on x with suint256
        x = 0;
        x += z;
        assert(x == 6);

        // casting
        x = 0x123456789;
        uint256 publicX = x;
        assert(publicX == 0x123456789);

        uint256 a = 0;
        a += x;
        assert(a == 0x123456789);

        suint256 b = 0;
        b += x;
        assert(b == 0x123456789);
        assert(x == 0x123456789);

        // explicit casting
        publicX = uint256(x);
        assert(publicX == 0x123456789);

        x = suint256(4);
        assert(x == 4);
        assert(x == suint256(4));
    }
}
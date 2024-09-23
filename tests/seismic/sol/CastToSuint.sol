contract CastToSuint {
    suint256 z;
    suint256 x;

    function test() public {
        // uint256 x = 0x123456789;
        // suint256 y = 0x123456789;
        z = 0x555;
        x = z;
    }
}